// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned group, linking and waypoint query views, never gameplay admission.
use super::*;
#[path = "groups_plans.rs"]
mod plan;
#[path = "groups_spawn.rs"]
mod spawn;
pub(crate) use spawn::SpawnGroupCatalogRows;
const LOADERS: &[&str] = &[
    "Globals/ObjectMgr:1125",
    "Globals/ObjectMgr:1192",
    "Globals/ObjectMgr:1250",
    "Globals/ObjectMgr:1355",
    "Globals/ObjectMgr:1398",
    "Globals/ObjectMgr:1426",
    "Entities/CreatureLinkingMgr:75",
    "Entities/CreatureLinkingMgr:120",
    "MotionGenerators/WaypointManager:173",
    "MotionGenerators/WaypointManager:198",
    "MotionGenerators/WaypointManager:302",
    "MotionGenerators/WaypointManager:329",
    "MotionGenerators/WaypointManager:400",
    "MotionGenerators/WaypointManager:427",
    "MotionGenerators/WaypointManager:441",
];
fn position(columns: &[Value], name: &str) -> Result<usize> {
    columns
        .iter()
        .position(|c| {
            c["COLUMN_NAME"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
        .ok_or_else(|| fail("catalog.schema"))
}
fn hash_cells<'a>(cells: impl IntoIterator<Item = Option<&'a [u8]>>) -> [u8; 32] {
    let mut h = Sha256::new();
    for cell in cells {
        h.update(cell.map_or(u32::MAX, |c| c.len() as u32).to_le_bytes());
        if let Some(c) = cell {
            h.update(c);
        }
    }
    h.finalize().into()
}
fn expected(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    spec: &Value,
) -> Result<(usize, Vec<[u8; 32]>)> {
    let table = spec["relations"][0]
        .as_str()
        .ok_or_else(|| fail("groups.query"))?;
    let query = spec["query"].as_str().ok_or_else(|| fail("groups.query"))?;
    let projection = query
        .strip_prefix("SELECT ")
        .and_then(|s| s.split_once(" FROM "))
        .ok_or_else(|| fail("groups.query"))?
        .0;
    let columns = schema["tables"][table]["columns"]
        .as_array()
        .ok_or_else(|| fail("catalog.schema"))?;
    let count_query = query.contains("GROUP BY");
    let names: Vec<_> = projection.split(", ").collect();
    let indexes: Vec<_> = names
        .iter()
        .filter(|n| !n.starts_with("COUNT("))
        .map(|n| position(columns, n))
        .collect::<Result<_>>()?;
    let point = if count_query {
        Some(position(columns, "Point")?)
    } else {
        None
    };
    let mut counts = BTreeMap::<Vec<u8>, u64>::new();
    let mut null_count = None;
    let mut hashes = Vec::new();
    catalog.scan(table, 0, 256, |row| {
        let mut cells = [None; 256];
        for (i, cell) in row.cells().enumerate() {
            cells[i] = cell?;
        }
        if let Some(p) = point {
            let count = u64::from(cells[p].is_some());
            if let Some(key) = cells[indexes[0]] {
                if let Some(existing) = counts.get_mut(key) {
                    *existing += count;
                } else {
                    counts.insert(key.to_vec(), count);
                }
            } else {
                *null_count.get_or_insert(0u64) += count;
            }
            check(counts.len() <= DECODED / 256, "groups.view_limit")?;
        } else {
            check(hashes.len() < DECODED / 128, "groups.view_limit")?;
            hashes.push(hash_cells(indexes.iter().map(|&i| cells[i])));
        }
        Ok(())
    })?;
    for (key, count) in counts {
        let count = count.to_string();
        hashes.push(hash_cells([Some(key.as_slice()), Some(count.as_bytes())]));
    }
    if let Some(count) = null_count {
        let text = count.to_string();
        hashes.push(hash_cells([None, Some(text.as_bytes())]));
    }
    hashes.sort_unstable();
    Ok((names.len(), hashes))
}
pub(super) fn read_stream_row<'a>(
    held: &reader::Held,
    buffer: &'a mut [u8],
    at: &mut u64,
    ordinal: u64,
    columns: usize,
) -> Result<RowView<'a>> {
    check(*at + 4 <= held.len(), "groups.exhausted")?;
    held.read_into(*at, &mut buffer[..4])?;
    let size = le(buffer, 0, 4)? as usize;
    check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
    held.read_into(*at + 4, &mut buffer[4..size])?;
    let row = RowView::new(&buffer[..size], ordinal, columns)?;
    row.validate()?;
    *at += size as u64;
    Ok(row)
}
fn verify_stream(path: &Path, columns: usize, expected: &[[u8; 32]], d: &Value) -> Result<Value> {
    let held = reader::Held::open(path, MAX_RAW)?;
    verify_held_stream(&held, columns, expected, d, &mut vec![0; FRAME])
}
fn verify_held_stream(
    held: &reader::Held,
    columns: usize,
    expected: &[[u8; 32]],
    d: &Value,
    buffer: &mut [u8],
) -> Result<Value> {
    let mut seen = Vec::with_capacity(expected.len());
    let mut hash = Sha256::new();
    let mut at = 0;
    for ordinal in 0..expected.len() {
        let row = read_stream_row(held, buffer, &mut at, ordinal as u64, columns)?;
        let mut cells = [None; 256];
        for (i, cell) in row.cells().enumerate() {
            cells[i] = cell?;
        }
        seen.push(hash_cells(cells[..columns].iter().copied()));
        hash.update(row.frame);
    }
    check(at == held.len(), "groups.trailing")?;
    let occurrence = hex(&hash.finalize());
    check(
        d["bytes"] == held.len() && d["rows"] == seen.len() as u64 && d["sha256"] == occurrence,
        "groups.hash",
    )?;
    seen.sort_unstable();
    check(seen == expected, "groups.view_mismatch")?;
    let mut multiset = Sha256::new();
    for h in seen {
        multiset.update(h);
    }
    held.check()?;
    Ok(
        json!({"rows":expected.len(), "occurrence_sha256":occurrence, "multiset_sha256":hex(&multiset.finalize())}),
    )
}
fn occurrence_manifest(occurrences: &Path) -> Result<(reader::Held, Value)> {
    let held = reader::Held::open(&occurrences.join("occurrences.json"), CHUNK as u64)?;
    let meta = bounded_json(&held.read(0, held.len() as usize, &mut Metrics::default(), CACHE)?)?;
    catalog_schema::fields(
        &meta,
        &["format", "streams", "complete", "source_snapshot"],
        "groups.occurrences",
    )?;
    check(
        meta["format"] == "benilla-group-occurrences-v1"
            && meta["complete"] == false
            && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1",
        "groups.occurrences",
    )?;
    let keys: Vec<_> = (0..LOADERS.len()).map(|i| i.to_string()).collect();
    catalog_schema::fields(
        &meta["streams"],
        &keys.iter().map(String::as_str).collect::<Vec<_>>(),
        "groups.occurrences",
    )?;
    Ok((held, meta))
}
/// Checked source-query row order, not effective definitions or sealed F1 inputs.
pub(crate) struct LinkingCatalogRows {
    metadata: reader::Held,
    streams: [reader::Held; 2],
    rows: [u64; 2],
    at: [u64; 2],
    ordinal: [u64; 2],
    buffer: Vec<u8>,
}
impl LinkingCatalogRows {
    pub(super) fn open(catalog: &mut VerifiedCatalog, occurrences: &Path) -> Result<Self> {
        let (metadata, meta) = occurrence_manifest(occurrences)?;
        let schema =
            bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
        let mut streams = [None, None];
        let mut rows = [0; 2];
        let mut buffer = vec![0; FRAME];
        for (slot, loader) in [6, 7].into_iter().enumerate() {
            let spec = &catalog_schema::POLICY["loaders"][LOADERS[loader]];
            let d = &meta["streams"][loader.to_string()];
            catalog_schema::fields(d, &["query", "rows", "bytes", "sha256"], "groups.query")?;
            check(
                d["query"] == spec["query"]
                    && d["rows"].as_u64().is_some_and(|n| n <= 2_000_000)
                    && d["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW),
                "groups.query",
            )?;
            let (columns, wanted) = expected(catalog, &schema, spec)?;
            check(columns == if slot == 0 { 5 } else { 3 }, "groups.query")?;
            let held = reader::Held::open(&occurrences.join(format!("{loader}.rows")), MAX_RAW)?;
            verify_held_stream(&held, columns, &wanted, d, &mut buffer)?;
            rows[slot] = wanted.len() as u64;
            streams[slot] = Some(held);
        }
        metadata.check()?;
        let [first, second] = streams;
        let streams = [
            first.ok_or_else(|| fail("groups.query"))?,
            second.ok_or_else(|| fail("groups.query"))?,
        ];
        Ok(Self {
            metadata,
            streams,
            rows,
            at: [0; 2],
            ordinal: [0; 2],
            buffer,
        })
    }
    fn next<const N: usize>(&mut self, slot: usize) -> Result<Option<(u64, [u32; N])>> {
        self.metadata.check()?;
        self.streams[slot].check()?;
        if slot == 1 {
            check(self.ordinal[0] == self.rows[0], "groups.loader_phase")?;
        }
        if self.ordinal[slot] == self.rows[slot] {
            check(self.at[slot] == self.streams[slot].len(), "groups.trailing")?;
            return Ok(None);
        }
        let ordinal = self.ordinal[slot];
        let row = read_stream_row(
            &self.streams[slot],
            &mut self.buffer,
            &mut self.at[slot],
            ordinal,
            N,
        )?;
        let mut values = [0; N];
        for (value, cell) in values.iter_mut().zip(row.cells()) {
            *value = spawns::uint(cell?)?;
        }
        self.ordinal[slot] += 1;
        Ok(Some((ordinal, values)))
    }
    pub(crate) fn next_template(&mut self) -> Result<Option<(u64, [u32; 5])>> {
        self.next(0)
    }
    pub(crate) fn next_guid(&mut self) -> Result<Option<(u64, [u32; 3])>> {
        self.next(1)
    }
}
/// Recomputes every loader projection and grouped count from checked source rows.
pub fn diagnose_catalog_groups(root: &Path, occurrences: &Path) -> Result<Value> {
    let mut catalog = VerifiedCatalog::open(root, false)?;
    prepare(&mut catalog, occurrences)
}
pub(super) fn prepare(catalog: &mut VerifiedCatalog, occurrences: &Path) -> Result<Value> {
    let schema = bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
    let (held, meta) = occurrence_manifest(occurrences)?;
    let mut loaders = serde_json::Map::new();
    let mut families = BTreeMap::new();
    for (i, id) in LOADERS.iter().enumerate() {
        let spec = &catalog_schema::POLICY["loaders"][id];
        let key = i.to_string();
        let d = &meta["streams"][&key];
        catalog_schema::fields(d, &["query", "rows", "bytes", "sha256"], "groups.query")?;
        check(d["query"] == spec["query"], "groups.query")?;
        check(
            d["rows"].as_u64().is_some_and(|n| n <= 2_000_000)
                && d["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW),
            "groups.hash",
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
        families.insert(
            spec["relations"][0]
                .as_str()
                .ok_or_else(|| fail("groups.query"))?
                .to_owned(),
            Value::Null,
        );
    }
    for (table, report) in &mut families {
        let columns = schema["tables"][table]["columns"]
            .as_array()
            .ok_or_else(|| fail("catalog.schema"))?;
        let floats: Vec<_> = columns
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                matches!(
                    c["DATA_TYPE"].as_str(),
                    Some("float" | "double" | "decimal")
                )
            })
            .map(|(i, _)| i)
            .collect();
        let mut raw = Sha256::new();
        let mut bits = Sha256::new();
        let mut count = 0u64;
        catalog.scan(table, 0, 256, |row| {
            raw.update(row.frame);
            let mut cells = [None; 256];
            for (i, cell) in row.cells().enumerate() {
                cells[i] = cell?;
            }
            for &i in &floats {
                bits.update(row.ordinal.to_le_bytes());
                bits.update((i as u32).to_le_bytes());
                bits.update(super::spawns::floating(cells[i])?.to_bits().to_le_bytes());
            }
            count += 1;
            Ok(())
        })?;
        *report = json!({"rows":count,"raw_sha256":hex(&raw.finalize()),"float_bits_sha256":hex(&bits.finalize())});
    }
    let mut report = json!({"format":"benilla-group-views-v1","catalog_sha256":catalog.manifest["catalog_sha256"],"families":families,"loaders":loaders,"complete":false,"refusals":["groups.reference_execution_missing","groups.gameplay_not_admitted"]});
    report["plans"] = plan::plans(catalog, &schema)?;
    report["view_sha256"] = Value::String(json::digest(&report));
    held.check()?;
    Ok(report)
}

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
