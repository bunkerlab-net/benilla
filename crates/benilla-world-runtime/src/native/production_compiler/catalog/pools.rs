// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete PoolManager input occurrences and validation candidates, never random outcomes.
use super::spawns::{columns, fill, floating, hash_cells, position, uint, visit_key};
use super::*;
use std::collections::BTreeSet;
const MAX_ROWS: u64 = 2_000_000;

const TABLES: [&str; 6] = [
    "pool_template",
    "pool_creature",
    "pool_creature_template",
    "pool_gameobject",
    "pool_gameobject_template",
    "pool_pool",
];
const STREAMS: [&str; 7] = [
    "maximum",
    "pool_template",
    "pool_creature",
    "pool_creature_template",
    "pool_gameobject",
    "pool_gameobject_template",
    "pool_pool",
];
const LINES: [u32; 7] = [606, 618, 656, 716, 792, 853, 927];
const COLS: [&[&str]; 7] = [
    &["maximum"],
    &["entry", "max_limit", "description"],
    &["guid", "pool_entry", "chance"],
    &["guid", "pool_entry", "chance", "id"],
    &["guid", "pool_entry", "chance"],
    &["guid", "pool_entry", "chance", "id"],
    &["pool_id", "mother_pool", "chance"],
];

type PoolMember = (u32, f32, bool);

#[derive(Default)]
struct State {
    maximum: u32,
    maps: BTreeSet<u32>,
    templates: BTreeMap<u32, u32>,
    pool_maps: BTreeMap<u32, u32>,
    search: BTreeMap<(&'static str, u32), u32>,
    parents: BTreeMap<u32, u32>,
    nested: BTreeSet<u32>,
    groups: BTreeMap<(&'static str, u32), Vec<PoolMember>>,
    records: Vec<Value>,
    budget: Option<usize>,
}
impl State {
    fn emit(&mut self, record: Value) -> Result<()> {
        check(self.records.len() < MAX_ROWS as usize, "pool.row_limit")?;
        let budget = self.budget.unwrap_or(DECODED);
        check(
            (self.records.len() + 1) * 640 <= budget,
            "pool.decoded_limit",
        )?;
        self.records.push(record);
        Ok(())
    }
    fn map_check(&mut self, pool: u32, map: u32) -> Option<&'static str> {
        if !self.maps.contains(&map) {
            return Some("missing_map");
        }
        if self.pool_maps.get(&pool).is_some_and(|m| *m != map) {
            return Some("cross_map");
        }
        self.pool_maps.insert(pool, map);
        None
    }
    fn apply(
        &mut self,
        name: &str,
        ordinal: u64,
        cells: &[Option<&[u8]>],
        map: Option<u32>,
    ) -> Result<()> {
        if name == "maximum" {
            return Ok(());
        }
        if name == "pool_template" {
            let pool = uint(cells[0])? & 65535;
            check(pool <= self.maximum, "pool.maximum_range")?;
            let limit = uint(cells[1])?;
            self.templates.insert(pool, limit);
            return self.emit(json!([name, ordinal, "template", pool, limit]));
        }
        let mut guid = uint(cells[0])?;
        let pool = uint(cells[1])? & 65535;
        let chance = floating(cells[2])?;
        let mut status = None;
        let kind = if name == "pool_pool" {
            "pool"
        } else if name.contains("creature") {
            "creature"
        } else {
            "gameobject"
        };
        if kind == "pool" {
            guid &= 65535;
            status = if pool > self.maximum {
                Some("mother_out_of_range")
            } else if guid > self.maximum {
                Some("child_out_of_range")
            } else if guid == pool {
                Some("self_cycle")
            } else {
                None
            };
        } else if map.is_none() {
            status = Some("missing_spawn");
        } else if pool > self.maximum {
            status = Some("pool_out_of_range");
        }
        if status.is_none() && (chance < 0.0 || chance > 100.0) {
            status = Some("invalid_chance");
        }
        if kind != "pool" && status.is_none() {
            if name.ends_with("_template")
                && self.search.get(&(kind, guid)).is_some_and(|p| *p != 0)
            {
                status = Some("duplicate_guid");
            } else {
                status = self.map_check(pool, map.ok_or_else(|| fail("pool.missing_spawn"))?);
            }
        }
        if status.is_none() {
            // Nonzero chance is explicit only with MaxLimit=1, PoolManager.cpp:139-159.
            let explicit = chance != 0.0 && self.templates.get(&pool) == Some(&1);
            self.groups
                .entry((kind, pool))
                .or_default()
                .push((guid, chance, explicit));
            if kind == "pool" {
                self.parents.entry(guid).or_insert(pool);
                self.nested.insert(guid);
            } else {
                self.search.entry((kind, guid)).or_insert(pool);
            }
            status = Some("candidate_accepted");
        }
        self.emit(json!([name, ordinal, status, guid, pool, chance.to_bits()]))
    }
    fn finish(&mut self) -> Result<Value> {
        // The source excludes MAX from this traversal, PoolManager.cpp:988-1021.
        for start in 0..self.maximum {
            let mut checked = BTreeSet::new();
            let mut child = start;
            while let Some(&parent) = self.parents.get(&child) {
                let mut status = self
                    .pool_maps
                    .get(&child)
                    .copied()
                    .and_then(|m| self.map_check(parent, m));
                checked.insert(child);
                if status.is_none() && checked.contains(&parent) {
                    status = Some("cycle_removed");
                }
                if let Some(status) = status {
                    self.emit(json!(["hierarchy", start, status, child, parent]))?;
                    self.parents.remove(&child);
                    let group = self
                        .groups
                        .get_mut(&("pool", parent))
                        .ok_or_else(|| fail("pool.hierarchy"))?;
                    for explicit in [true, false] {
                        if let Some(i) = group.iter().position(|r| r.0 == child && r.2 == explicit)
                        {
                            group.remove(i);
                        }
                    }
                    break;
                }
                child = parent;
            }
        }
        let mut disabled = Vec::new();
        let templates: Vec<_> = self.templates.keys().copied().collect();
        for pool in templates {
            if self.nested.contains(&pool) {
                continue;
            }
            for kind in ["gameobject", "creature", "pool"] {
                let group = self
                    .groups
                    .get(&(kind, pool))
                    .map_or(&[][..], Vec::as_slice);
                if group.iter().any(|r| !r.2) {
                    continue;
                }
                let total = group.iter().fold(0.0f32, |sum, r| sum + r.1);
                if total != 0.0 && total != 100.0 {
                    disabled.push(pool);
                    self.emit(json!([
                        "integrity",
                        pool,
                        "autospawn_disabled",
                        kind,
                        total.to_bits()
                    ]))?;
                    break;
                }
            }
        }
        Ok(
            json!({"parents": self.parents.iter().map(|(c,p)| [*c,*p]).collect::<Vec<_>>(),
            "maps": self.pool_maps.iter().map(|(p,m)| [*p,*m]).collect::<Vec<_>>(), "autospawn_disabled": disabled}),
        )
    }
}

fn projection(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    name: &str,
    i: usize,
    mut f: impl FnMut(&[Option<&[u8]>]) -> Result<()>,
) -> Result<()> {
    let cols = columns(schema, name)?;
    let positions = COLS[i]
        .iter()
        .map(|c| position(&cols, c))
        .collect::<Result<Vec<_>>>()?;
    catalog.scan(name, 0, 256, |row| {
        let mut cells = [None; 256];
        fill(&row, &mut cells)?;
        let mut projected = [None; 4];
        for (j, &p) in positions.iter().enumerate() {
            projected[j] = cells[p];
        }
        f(&projected[..positions.len()])
    })
}
fn maximum(catalog: &mut VerifiedCatalog, schema: &Value) -> Result<Option<u32>> {
    let mut value = None;
    projection(catalog, schema, "pool_template", 1, |c| {
        let n = uint(c[0])?;
        value = Some(value.map_or(n, |v: u32| v.max(n)));
        Ok(())
    })?;
    Ok(value)
}
fn push_hash(hashes: &mut Vec<[u8; 32]>, cells: &[Option<&[u8]>], budget: usize) -> Result<()> {
    check(hashes.len() < MAX_ROWS as usize, "pool.join_limit")?;
    check((hashes.len() + 1) * 256 <= budget, "pool.decoded_limit")?;
    hashes.push(hash_cells(cells));
    Ok(())
}
fn expected(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    i: usize,
    budget: usize,
) -> Result<Vec<[u8; 32]>> {
    let name = STREAMS[i];
    let mut hashes = Vec::new();
    if i == 0 {
        let text = maximum(catalog, schema)?.map(|v| v.to_string());
        push_hash(&mut hashes, &[text.as_ref().map(|v| v.as_bytes())], budget)?;
    } else if i == 3 || i == 5 {
        let kind = if i == 3 { "creature" } else { "gameobject" };
        let cols = columns(schema, name)?;
        let (id, pool, chance) = (
            position(&cols, "id")?,
            position(&cols, "pool_entry")?,
            position(&cols, "chance")?,
        );
        let mut by_entry = BTreeMap::<u32, Vec<[Option<Vec<u8>>; 3]>>::new();
        catalog.scan(name, 0, 256, |row| {
            let mut c = [None; 256];
            fill(&row, &mut c)?;
            for p in [id, pool, chance] {
                check(c[p].is_none_or(|v| v.len() < 128), "spawn.numeric_limit")?;
            }
            by_entry.entry(uint(c[id])?).or_default().push([
                c[pool].map(<[u8]>::to_vec),
                c[chance].map(<[u8]>::to_vec),
                c[id].map(<[u8]>::to_vec),
            ]);
            Ok(())
        })?;
        let cols = columns(schema, kind)?;
        let (id, guid) = (position(&cols, "id")?, position(&cols, "guid")?);
        let mut matched = BTreeSet::new();
        catalog.scan(kind, 0, 256, |row| {
            let mut c = [None; 256];
            fill(&row, &mut c)?;
            let entry = uint(c[id])?;
            if let Some(tails) = by_entry.get(&entry) {
                matched.insert(entry);
                for tail in tails {
                    push_hash(
                        &mut hashes,
                        &[
                            c[guid],
                            tail[0].as_deref(),
                            tail[1].as_deref(),
                            tail[2].as_deref(),
                        ],
                        budget,
                    )?;
                }
            }
            Ok(())
        })?;
        for (entry, tails) in by_entry {
            if !matched.contains(&entry) {
                for tail in tails {
                    push_hash(
                        &mut hashes,
                        &[
                            None,
                            tail[0].as_deref(),
                            tail[1].as_deref(),
                            tail[2].as_deref(),
                        ],
                        budget,
                    )?;
                }
            }
        }
    } else {
        projection(catalog, schema, name, i, |c| {
            push_hash(&mut hashes, c, budget)
        })?;
    }
    check(
        hashes.len() <= MAX_ROWS as usize && hashes.len() * 256 <= budget,
        "pool.join_limit",
    )?;
    hashes.sort_unstable();
    Ok(hashes)
}
fn maps(catalog: &mut VerifiedCatalog) -> Result<BTreeSet<u32>> {
    let d = descriptor(&catalog.manifest["dbc_root"])?;
    let dbcs = bounded_json(&catalog.object(&d, 0)?)?;
    check(dbcs.get("Map.dbc").is_some(), "pool.map_dbc_missing")?;
    let d = descriptor(&dbcs["Map.dbc"])?;
    let raw = catalog.object(&d, 0)?;
    check(raw.len() >= 20 && &raw[..4] == b"WDBC", "pool.map_dbc")?;
    let (count, fields, width, strings) = (
        le(&raw, 4, 4)?,
        le(&raw, 8, 4)?,
        le(&raw, 12, 4)?,
        le(&raw, 16, 4)?,
    );
    check(
        fields >= 1 && width == fields * 4 && 20 + count * width + strings == raw.len() as u64,
        "pool.map_dbc",
    )?;
    (0..count)
        .map(|i| Ok(le(&raw, (20 + i * width) as usize, 4)? as u32))
        .collect()
}
fn spawn_map(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    kind: &str,
    guid: u32,
) -> Result<Option<u32>> {
    let cols = columns(schema, kind)?;
    let map = position(&cols, "map")?;
    let mut found = None;
    visit_key(catalog, kind, guid, |row| {
        let mut cells = [None; 256];
        fill(&row, &mut cells)?;
        found = Some(uint(cells[map])?);
        Ok(())
    })?;
    Ok(found)
}
fn verify_stream(
    held: &reader::Held,
    columns: usize,
    expected: &[[u8; 32]],
    d: &Value,
    buffer: &mut [u8],
    mut visit: impl FnMut(u64, &[Option<&[u8]>]) -> Result<()>,
) -> Result<Value> {
    let mut at = 0;
    let mut h = Sha256::new();
    let mut seen = Vec::with_capacity(expected.len());
    for ordinal in 0..expected.len() {
        check(at + 4 <= held.len(), "pool.exhausted")?;
        held.read_into(at, &mut buffer[..4])?;
        let size = le(buffer, 0, 4)? as usize;
        check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
        held.read_into(at + 4, &mut buffer[4..size])?;
        let row = RowView::new(&buffer[..size], ordinal as u64, columns)?;
        row.validate()?;
        let mut cells = [None; 256];
        fill(&row, &mut cells)?;
        let cells = &cells[..columns];
        seen.push(hash_cells(cells));
        h.update(&buffer[..size]);
        at += size as u64;
        visit(ordinal as u64, cells)?;
    }
    check(at == held.len(), "pool.trailing")?;
    let occurrence_hash = hex(&h.finalize());
    check(
        d["bytes"] == held.len()
            && d["rows"] == seen.len() as u64
            && d["sha256"] == occurrence_hash,
        "pool.hash",
    )?;
    seen.sort_unstable();
    check(seen == expected, "pool.join_mismatch")?;
    let mut multiset = Sha256::new();
    for hash in &seen {
        multiset.update(hash);
    }
    Ok(
        json!({"rows":seen.len(),"occurrence_sha256":occurrence_hash,
        "multiset_sha256":hex(&multiset.finalize())}),
    )
}

pub fn diagnose_catalog_pools(
    root: &Path,
    occurrences: &Path,
    records: Option<&Path>,
) -> Result<Value> {
    let mut catalog = VerifiedCatalog::open(root, false)?;
    let _spawns = SpawnViews::build(&mut catalog)?;
    let d = descriptor(&catalog.manifest["schema_root"])?;
    let schema = bounded_json(&catalog.object(&d, 0)?)?;
    let mut counts = BTreeMap::new();
    let mut raw_hashes = BTreeMap::new();
    let mut rows = 0;
    for name in TABLES {
        let relation = catalog
            .relations
            .get(name)
            .ok_or_else(|| fail("pool.missing_relation"))?;
        rows += relation.rows;
        check(rows * 768 <= DECODED as u64, "pool.decoded_limit")?;
        let mut h = Sha256::new();
        let mut count = 0u64;
        catalog.scan(name, 0, 256, |row| {
            h.update(row.frame);
            count += 1;
            Ok(())
        })?;
        counts.insert(name, count);
        raw_hashes.insert(name, hex(&h.finalize()));
    }
    let held = reader::Held::open(&occurrences.join("occurrences.json"), CHUNK as u64)?;
    let meta = bounded_json(&held.read(0, held.len() as usize, &mut Metrics::default(), CACHE)?)?;
    catalog_schema::fields(
        &meta,
        &["format", "streams", "source_snapshot", "complete"],
        "pool.occurrences",
    )?;
    check(
        meta["format"] == "benilla-pool-occurrences-v3"
            && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
            && meta["complete"] == false,
        "pool.occurrences",
    )?;
    catalog_schema::fields(&meta["streams"], &STREAMS, "pool.occurrences")?;
    let mut state = State {
        maximum: maximum(&mut catalog, &schema)?.unwrap_or(0) & 65535,
        maps: maps(&mut catalog)?,
        ..State::default()
    };
    let mut streams = BTreeMap::new();
    let mut buffer = vec![0u8; FRAME];
    let spawn_charge =
        (catalog.relations["creature"].rows + catalog.relations["gameobject"].rows) as usize * 48;
    for (i, name) in STREAMS.iter().enumerate() {
        let budget = DECODED
            .checked_sub(spawn_charge + state.records.len() * 640 + 2_097_152)
            .ok_or_else(|| fail("pool.decoded_limit"))?;
        let expected = expected(&mut catalog, &schema, i, budget)?;
        state.budget = Some(DECODED - spawn_charge - expected.len() * 256 - 2_097_152);
        let d = &meta["streams"][name];
        catalog_schema::fields(d, &["sha256", "bytes", "rows", "query"], "pool.query")?;
        let id = format!("Pools/PoolManager:{}", LINES[i]);
        check(
            d["query"] == catalog_schema::POLICY["loaders"][&id]["query"],
            "pool.query",
        )?;
        check(
            d["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW)
                && d["rows"].as_u64().is_some_and(|n| n <= MAX_ROWS),
            "pool.hash",
        )?;
        let held = reader::Held::open(&occurrences.join(format!("{name}.rows")), MAX_RAW)?;
        let stream = verify_stream(
            &held,
            COLS[i].len(),
            &expected,
            d,
            &mut buffer,
            |ordinal, cells| {
                let map = if (2..=5).contains(&i) {
                    spawn_map(
                        &mut catalog,
                        &schema,
                        if i <= 3 { "creature" } else { "gameobject" },
                        uint(cells[0])?,
                    )?
                } else {
                    None
                };
                state.apply(name, ordinal, cells, map)
            },
        )?;
        streams.insert(*name, stream);
    }
    let hierarchy = state.finish()?;
    let mut disposition_hash = Sha256::new();
    let mut dispositions = BTreeMap::<String, u64>::new();
    for record in &state.records {
        disposition_hash.update(record.to_string().as_bytes());
        disposition_hash.update(b"\n");
        *dispositions
            .entry(
                record[2]
                    .as_str()
                    .ok_or_else(|| fail("pool.disposition"))?
                    .to_owned(),
            )
            .or_default() += 1;
    }
    let hierarchy_counts: BTreeMap<_, _> = hierarchy
        .as_object()
        .ok_or_else(|| fail("pool.hierarchy"))?
        .iter()
        .map(|(k, v)| (k, v.as_array().map_or(0, Vec::len)))
        .collect();
    let mut report = json!({"format":"benilla-pool-views-v3","catalog_sha256":catalog.manifest["catalog_sha256"],"counts":counts,"raw_sha256":raw_hashes,"streams":streams,"dispositions":dispositions,"disposition_sha256":hex(&disposition_hash.finalize()),"hierarchy_sha256":json::digest(&hierarchy),"hierarchy_counts":hierarchy_counts,"complete":false,"refusals":["pool.reference_execution_missing","pool.gameplay_not_admitted","pool.objectmgr_loaded_spawn_validation_pending"]});
    report["view_sha256"] = json!(json::digest(&report));
    if let Some(path) = records {
        use std::io::Write;
        std::fs::create_dir(path).map_err(|_| fail("pool.records"))?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join("dispositions.jsonl"))
            .map_err(|_| fail("pool.records"))?;
        for record in &state.records {
            writeln!(output, "{record}").map_err(|_| fail("pool.records"))?;
        }
        output.sync_all().map_err(|_| fail("pool.records"))?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join("hierarchy.json"))
            .map_err(|_| fail("pool.records"))?;
        output
            .write_all(hierarchy.to_string().as_bytes())
            .map_err(|_| fail("pool.records"))?;
        output.sync_all().map_err(|_| fail("pool.records"))?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_budget_refuses_before_retaining_a_record() {
        let mut state = State {
            budget: Some(1),
            ..State::default()
        };
        assert_eq!(
            state.emit(json!(["pool", 0, "candidate"])).unwrap_err().id,
            "pool.decoded_limit"
        );
        assert!(state.records.is_empty());
        assert_eq!(
            push_hash(&mut Vec::new(), &[None], 0).unwrap_err().id,
            "pool.decoded_limit"
        );
    }
    #[test]
    fn hierarchy_cross_map_removes_each_chance_list_but_keeps_nested_flag() {
        let mut state = State {
            maximum: 3,
            maps: BTreeSet::from([0, 1]),
            ..State::default()
        };
        state
            .apply(
                "pool_template",
                0,
                &[Some(b"3"), Some(b"1"), Some(b"parent")],
                None,
            )
            .unwrap();
        state.pool_maps.insert(1, 0);
        state.pool_maps.insert(3, 1);
        state
            .apply("pool_pool", 0, &[Some(b"1"), Some(b"3"), Some(b"50")], None)
            .unwrap();
        state
            .apply("pool_pool", 1, &[Some(b"1"), Some(b"3"), Some(b"0")], None)
            .unwrap();
        let hierarchy = state.finish().unwrap();
        assert_eq!(hierarchy["parents"], json!([]));
        assert!(state.groups[&("pool", 3)].is_empty());
        assert!(state.nested.contains(&1));
        assert!(state
            .records
            .iter()
            .any(|r| r[0] == "hierarchy" && r[2] == "cross_map"));
    }
    fn frame(ordinal: u64, cells: &[Option<&[u8]>]) -> Vec<u8> {
        let bitmap = cells.len().div_ceil(8);
        let mut raw = vec![0u8; 16 + bitmap];
        raw[4..12].copy_from_slice(&ordinal.to_le_bytes());
        raw[12..14].copy_from_slice(&(cells.len() as u16).to_le_bytes());
        raw[14..16].copy_from_slice(&(bitmap as u16).to_le_bytes());
        for (i, cell) in cells.iter().enumerate() {
            if let Some(cell) = cell {
                raw.extend_from_slice(&(cell.len() as u32).to_le_bytes());
                raw.extend_from_slice(cell);
            } else {
                raw[16 + i / 8] |= 1 << (i % 8);
            }
        }
        let size = raw.len() as u32;
        raw[..4].copy_from_slice(&size.to_le_bytes());
        raw
    }
    fn run(raw: &[u8], columns: usize, expected: &[[u8; 32]]) -> Result<Value> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pool-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, raw).unwrap();
        drop(file);
        let held = reader::Held::open(&path, MAX_RAW).unwrap();
        let d = json!({"sha256":hex(&Sha256::digest(raw)),"bytes":raw.len(),"rows":expected.len()});
        let result = verify_stream(&held, columns, expected, &d, &mut vec![0; FRAME], |_, _| {
            Ok(())
        });
        std::fs::remove_file(path).unwrap();
        result
    }
    #[test]
    fn absent_max_is_not_null_and_join_multiplicity_is_exact() {
        let null_hash: [u8; 32] = Sha256::digest([255u8; 4]).into();
        assert_eq!(run(&[], 1, &[null_hash]).unwrap_err().id, "pool.exhausted");
        assert_eq!(run(&frame(0, &[None]), 1, &[null_hash]).unwrap()["rows"], 1);
        let cell = [
            Some(b"7".as_slice()),
            Some(b"1".as_slice()),
            Some(b"0".as_slice()),
            Some(b"10".as_slice()),
        ];
        let null = [None, cell[1], cell[2], Some(b"99".as_slice())];
        let hash: [u8; 32] =
            Sha256::digest(b"\x01\0\0\x007\x01\0\0\x001\x01\0\0\x000\x02\0\0\x0010").into();
        let nhash: [u8; 32] =
            Sha256::digest(b"\xff\xff\xff\xff\x01\0\0\x001\x01\0\0\x000\x02\0\0\x0099").into();
        let mut expected = [hash, hash, nhash];
        expected.sort_unstable();
        let raw = [frame(0, &cell), frame(1, &null), frame(2, &cell)].concat();
        let reordered = [frame(0, &null), frame(1, &cell), frame(2, &cell)].concat();
        let a = run(&raw, 4, &expected).unwrap();
        let b = run(&reordered, 4, &expected).unwrap();
        assert_eq!(a["multiset_sha256"], b["multiset_sha256"]);
        assert_ne!(a["occurrence_sha256"], b["occurrence_sha256"]);
        assert_eq!(
            run(&[raw.as_slice(), b"x"].concat(), 4, &expected)
                .unwrap_err()
                .id,
            "pool.trailing"
        );
        assert_eq!(
            run(&raw[..raw.len() - 1], 4, &expected).unwrap_err().id,
            "catalog.truncated"
        );
        let forged = [frame(0, &cell), frame(1, &null), frame(2, &null)].concat();
        assert_eq!(
            run(&forged, 4, &expected).unwrap_err().id,
            "pool.join_mismatch"
        );
        let ordinal = [frame(0, &cell), frame(0, &null), frame(2, &cell)].concat();
        assert_eq!(
            run(&ordinal, 4, &expected).unwrap_err().id,
            "catalog.duplicate_row"
        );
    }
    #[test]
    fn zero_chance_and_multi_limit_use_equal_lists_and_holes_keep_defaults() {
        let mut s = State {
            maximum: 5,
            maps: BTreeSet::from([0]),
            ..State::default()
        };
        s.apply(
            "pool_template",
            0,
            &[Some(b"1"), Some(b"2"), Some(b"two")],
            None,
        )
        .unwrap();
        for (guid, pool, chance) in [
            ("1", "1", "70"),
            ("2", "1", "70"),
            ("3", "2", "0"),
            ("4", "5", "-0"),
        ] {
            s.apply(
                "pool_creature",
                0,
                &[
                    Some(guid.as_bytes()),
                    Some(pool.as_bytes()),
                    Some(chance.as_bytes()),
                ],
                Some(0),
            )
            .unwrap();
        }
        s.apply(
            "pool_creature",
            0,
            &[Some(b"1"), Some(b"1"), Some(b"101")],
            Some(0),
        )
        .unwrap();
        assert_eq!(s.records.last().unwrap()[2], "invalid_chance");
        s.apply("pool_pool", 0, &[Some(b"5"), Some(b"5"), Some(b"0")], None)
            .unwrap();
        assert_eq!(s.records.last().unwrap()[2], "self_cycle");
        assert!(s.groups.values().all(|g| g.iter().all(|r| !r.2)));
        assert_eq!(s.finish().unwrap()["autospawn_disabled"], json!([]));
        assert_eq!(s.map_check(1, 999), Some("missing_map"));
    }
    #[test]
    fn source_chances_cycles_and_cross_map_candidates() {
        let mut s = State {
            maximum: 4,
            maps: BTreeSet::from([0, 1]),
            ..State::default()
        };
        s.apply(
            "pool_template",
            0,
            &[Some(b"1"), Some(b"1"), Some(b"one")],
            None,
        )
        .unwrap();
        s.apply(
            "pool_creature",
            0,
            &[Some(b"7"), Some(b"1"), Some(b"70")],
            Some(0),
        )
        .unwrap();
        s.apply(
            "pool_creature",
            1,
            &[Some(b"8"), Some(b"1"), Some(b"70")],
            Some(0),
        )
        .unwrap();
        s.apply(
            "pool_creature_template",
            0,
            &[Some(b"7"), Some(b"1"), Some(b"0"), Some(b"10")],
            Some(0),
        )
        .unwrap();
        assert_eq!(s.records.last().unwrap()[2], "duplicate_guid");
        s.apply(
            "pool_gameobject",
            0,
            &[Some(b"7"), Some(b"1"), Some(b"0")],
            Some(1),
        )
        .unwrap();
        assert_eq!(s.records.last().unwrap()[2], "cross_map");
        s.apply(
            "pool_creature",
            2,
            &[Some(b"9"), Some(b"1"), Some(b"-1")],
            Some(0),
        )
        .unwrap();
        assert_eq!(s.records.last().unwrap()[2], "invalid_chance");
        for (child, parent) in [(2, 3), (3, 2)] {
            let c = child.to_string();
            let p = parent.to_string();
            s.apply(
                "pool_pool",
                0,
                &[Some(c.as_bytes()), Some(p.as_bytes()), Some(b"0")],
                None,
            )
            .unwrap();
        }
        let h = s.finish().unwrap();
        assert_eq!(h["parents"], json!([[2, 3]]));
        assert_eq!(h["autospawn_disabled"], json!([1]));
        assert!(s.records.iter().any(|r| r[2] == "cycle_removed"));
    }
}
