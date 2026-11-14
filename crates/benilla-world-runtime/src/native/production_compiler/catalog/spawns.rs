// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete source views. Index order is never loader occurrence order.
use super::*;
use std::collections::BTreeSet;
#[path = "effective_spawns.rs"]
mod effective_spawns;
pub use effective_spawns::{ObjectMgrSpawnRow, ObjectMgrSpawnRows};
const FAMILIES: &[&str] = &[
    "creature",
    "gameobject",
    "creature_spawn_entry",
    "gameobject_spawn_entry",
    "creature_template",
    "gameobject_template",
    "creature_model_info",
    "creature_equip_template",
    "creature_template_classlevelstats",
    "creature_template_addon",
    "creature_addon",
    "gameobject_addon",
    "gameobject_template_addon",
    "creature_spawn_data",
    "creature_spawn_data_template",
    "creature_conditional_spawn",
    "creature_immunities",
    "creature_template_spells",
    "creature_spell_targeting",
    "creature_spell_list_entry",
    "creature_spell_list",
    "creature_cooldowns",
];
const CREATURE: &[&str] = &[
    "guid",
    "id",
    "map",
    "position_x",
    "position_y",
    "position_z",
    "orientation",
    "spawntimesecsmin",
    "spawntimesecsmax",
    "spawndist",
    "MovementType",
    "spawnMask",
];
const GO: &[&str] = &[
    "guid",
    "id",
    "map",
    "position_x",
    "position_y",
    "position_z",
    "orientation",
    "rotation0",
    "rotation1",
    "rotation2",
    "rotation3",
    "spawntimesecsmin",
    "spawntimesecsmax",
    "spawnMask",
];
type JoinSpec = (&'static str, &'static str, &'static str);
const CJOINS: &[JoinSpec] = &[
    ("game_event_creature", "guid", "event"),
    ("pool_creature", "guid", "pool_entry"),
    ("pool_creature_template", "id", "pool_entry"),
    ("creature_spawn_data", "guid", "id"),
];
const GJOINS: &[JoinSpec] = &[
    ("game_event_gameobject", "guid", "event"),
    ("pool_gameobject", "guid", "pool_entry"),
    ("pool_gameobject_template", "id", "pool_entry"),
];
fn numeric(raw: Option<&[u8]>) -> Result<&[u8]> {
    let raw = raw.unwrap_or(b"0");
    check(raw.len() < 128, "spawn.numeric_limit")?;
    Ok(raw
        .trim_ascii_start()
        .split(|b| *b == 0)
        .next()
        .unwrap_or(b""))
}
pub(super) fn uint(raw: Option<&[u8]>) -> Result<u32> {
    // Field::GetUInt32 casts atoll, src/shared/Database/Field.h:59.
    let raw = numeric(raw)?;
    let negative = raw.first() == Some(&b'-');
    let start = usize::from(matches!(raw.first(), Some(b'+' | b'-')));
    let limit = if negative {
        1u64 << 63
    } else {
        i64::MAX as u64
    };
    let mut value = 0u64;
    for &b in &raw[start..] {
        if !b.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(u64::from(b - b'0'))
            .min(limit);
    }
    Ok(if negative {
        0u32.wrapping_sub(value as u32)
    } else {
        value as u32
    })
}
pub(super) fn floating(raw: Option<&[u8]>) -> Result<f32> {
    // Parse SQL decimal text to double before casting, src/shared/Database/Field.h:53.
    let raw = numeric(raw)?;
    let mut end = usize::from(matches!(raw.first(), Some(b'+' | b'-')));
    let start = end;
    check(
        !raw.get(start..).is_some_and(|s| {
            s.starts_with(b"0x")
                || s.starts_with(b"0X")
                || s.get(..3).is_some_and(|p| {
                    p.eq_ignore_ascii_case(b"nan") || p.eq_ignore_ascii_case(b"inf")
                })
        }),
        "spawn.numeric_unsupported",
    )?;
    while raw.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let mut digits = end - start;
    if raw.get(end) == Some(&b'.') {
        end += 1;
        let fraction = end;
        while raw.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        digits += end - fraction;
    }
    if digits == 0 {
        return Ok(0.0);
    }
    if matches!(raw.get(end), Some(b'e' | b'E')) {
        let exponent = end;
        end += 1;
        if matches!(raw.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let first = end;
        while raw.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == first {
            end = exponent;
        }
    }
    let text = std::str::from_utf8(&raw[..end]).map_err(|_| fail("spawn.numeric_unsupported"))?;
    Ok(text
        .parse::<f64>()
        .map_err(|_| fail("spawn.numeric_unsupported"))? as f32)
}
fn coordinates(x: f32, y: f32) -> Result<(u32, u32)> {
    // Compute promotes float constants to double, Maps/GridDefines.h:147-166.
    let size = 533.333_3_f32;
    let cell_size = size / 16.0;
    let compute = |v: f32, s: f32, center: f64| -> Result<u32> {
        check(v.is_finite(), "spawn.invalid_coordinates")?;
        let result = ((f64::from(v) - f64::from(s / 2.0)) / f64::from(s) + center + 0.5).trunc();
        check(
            result >= 0.0 && result < center * 2.0,
            "spawn.invalid_coordinates",
        )?;
        Ok(result as u32)
    };
    let gx = compute(x, size, 32.0)?;
    let gy = compute(y, size, 32.0)?;
    let cx = compute(x, cell_size, 512.0)?;
    let cy = compute(y, cell_size, 512.0)?;
    Ok((gy * 64 + gx, cy * 1024 + cx))
}
pub(super) fn columns(schema: &Value, name: &str) -> Result<Vec<String>> {
    schema["tables"][name]["columns"]
        .as_array()
        .ok_or_else(|| fail("spawn.missing_relation"))?
        .iter()
        .map(|c| {
            c["COLUMN_NAME"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| fail("catalog.schema"))
        })
        .collect()
}
pub(super) fn position(cols: &[String], name: &str) -> Result<usize> {
    cols.iter()
        .position(|c| c.eq_ignore_ascii_case(name))
        .ok_or_else(|| fail("catalog.schema"))
}
pub(super) fn fill<'a>(row: &RowView<'a>, cells: &mut [Option<&'a [u8]>; 256]) -> Result<()> {
    check(row.columns <= cells.len(), "spawn.column_limit")?;
    for (i, cell) in row.cells().enumerate() {
        cells[i] = cell?;
    }
    Ok(())
}
pub(super) fn hash_cells(cells: &[Option<&[u8]>]) -> [u8; 32] {
    let mut h = Sha256::new();
    for cell in cells {
        h.update(cell.map_or(u32::MAX, |v| v.len() as u32).to_le_bytes());
        if let Some(v) = cell {
            h.update(v);
        }
    }
    h.finalize().into()
}
#[derive(Clone)]
struct NumericCell {
    data: [u8; 32],
    len: Option<u8>,
}
impl NumericCell {
    fn new(raw: Option<&[u8]>) -> Result<Self> {
        let mut data = [0; 32];
        let len = if let Some(v) = raw {
            check(v.len() <= data.len(), "spawn.numeric_limit")?;
            data[..v.len()].copy_from_slice(v);
            Some(v.len() as u8)
        } else {
            None
        };
        Ok(Self { data, len })
    }
    fn raw(&self) -> Option<&[u8]> {
        self.len.map(|n| &self.data[..usize::from(n)])
    }
}
/// Numeric map/grid indexes contain only source ordinal references.
pub struct SpawnViews {
    indexes: BTreeMap<String, Vec<[u8; 24]>>,
    report: Value,
    schema: Value,
}
impl SpawnViews {
    pub fn build(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let d = descriptor(&catalog.manifest["schema_root"])?;
        let schema = bounded_json(&catalog.object(&d, 0)?)?;
        let mut indexes = BTreeMap::<String, Vec<[u8; 24]>>::new();
        let mut counts = serde_json::Map::new();
        let mut map_counts = BTreeMap::<u32, [u64; 2]>::new();
        let mut raw_hashes = serde_json::Map::new();
        let mut bits_hashes = serde_json::Map::new();
        let mut entry_zero = 0u64;
        let mut invalid_coordinates = 0u64;
        for &name in FAMILIES {
            let cols = columns(&schema, name)?;
            let floats: Vec<_> = schema["tables"][name]["columns"]
                .as_array()
                .expect("columns")
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    matches!(
                        c["DATA_TYPE"].as_str(),
                        Some("decimal" | "float" | "double")
                    )
                })
                .map(|(i, _)| i)
                .collect();
            let spatial = matches!(name, "creature" | "gameobject");
            let spatial_cols = if spatial {
                Some([
                    position(&cols, "map")?,
                    position(&cols, "guid")?,
                    position(&cols, "position_x")?,
                    position(&cols, "position_y")?,
                    position(&cols, "id")?,
                ])
            } else {
                None
            };
            let mut raw = Sha256::new();
            let mut bits = Sha256::new();
            let mut count = 0u64;
            let prior: usize = indexes.values().map(|v| v.capacity() * 24).sum();
            let capacity = if spatial {
                let rows = catalog
                    .relations
                    .get(name)
                    .ok_or_else(|| fail("spawn.missing_relation"))?
                    .rows;
                check(
                    rows <= 2_000_000 && rows * 24 + prior as u64 <= DECODED as u64,
                    "spawn.index_limit",
                )?;
                rows as usize
            } else {
                0
            };
            let mut records = Vec::with_capacity(capacity);
            catalog.scan(name, 0, 256, |row| {
                let mut cells = [None; 256];
                fill(&row, &mut cells)?;
                raw.update(row.frame);
                for &i in &floats {
                    bits.update(row.ordinal.to_le_bytes());
                    bits.update((i as u32).to_le_bytes());
                    bits.update(floating(cells[i])?.to_bits().to_le_bytes());
                }
                if let Some([m, guid, x, y, entry]) = spatial_cols {
                    let map = uint(cells[m])?;
                    map_counts.entry(map).or_default()[usize::from(name == "gameobject")] += 1;
                    entry_zero += u64::from(uint(cells[entry])? == 0);
                    let (grid, cell) = match coordinates(floating(cells[x])?, floating(cells[y])?) {
                        Ok(pair) => pair,
                        Err(e) if e.id == "spawn.invalid_coordinates" => {
                            invalid_coordinates += 1;
                            count += 1;
                            return Ok(());
                        }
                        Err(e) => return Err(e),
                    };
                    let mut record = [0; 24];
                    record[..4].copy_from_slice(&map.to_be_bytes());
                    record[4..8].copy_from_slice(&grid.to_be_bytes());
                    record[8..12].copy_from_slice(&cell.to_be_bytes());
                    record[12..16].copy_from_slice(&uint(cells[guid])?.to_be_bytes());
                    record[16..].copy_from_slice(&row.ordinal.to_be_bytes());
                    records.push(record);
                }
                count += 1;
                Ok(())
            })?;
            counts.insert(name.into(), json!(count));
            raw_hashes.insert(name.into(), json!(hex(&raw.finalize())));
            bits_hashes.insert(name.into(), json!(hex(&bits.finalize())));
            if spatial {
                records.sort_unstable();
                indexes.insert(name.into(), records);
            }
        }
        for (map, [creatures, gameobjects]) in map_counts {
            counts.insert(
                map.to_string(),
                json!({"creature":creatures,"gameobject":gameobjects}),
            );
        }
        let index_hashes: BTreeMap<_, _> = indexes
            .iter()
            .map(|(name, records)| {
                let mut h = Sha256::new();
                for r in records {
                    h.update(r);
                }
                (name.clone(), hex(&h.finalize()))
            })
            .collect();
        let report = json!({"format":"benilla-spawn-views-v3","catalog_sha256":catalog.manifest["catalog_sha256"],
            "counts":counts,"raw_sha256":raw_hashes,"float_bits_sha256":bits_hashes,"indexes":index_hashes,
            "joined":{},"dispositions":{"entry_zero":entry_zero,"invalid_coordinates":invalid_coordinates},
            "complete":false,"refusals":["spawn.reference_execution_missing","spawn.gameplay_not_admitted"]});
        Ok(Self {
            indexes,
            report,
            schema,
        })
    }
    pub fn map_rows(
        &self,
        catalog: &mut VerifiedCatalog,
        name: &str,
        map: u32,
        grid: Option<u32>,
        mut f: impl FnMut(RowView<'_>) -> Result<()>,
    ) -> Result<()> {
        let rows = self.indexes.get(name).ok_or_else(|| fail("spawn.key"))?;
        check(grid.is_none_or(|g| g < 4096), "spawn.key")?;
        let mut prefix = [0u8; 8];
        prefix[..4].copy_from_slice(&map.to_be_bytes());
        let n = if let Some(g) = grid {
            prefix[4..].copy_from_slice(&g.to_be_bytes());
            8
        } else {
            4
        };
        let first = rows.partition_point(|r| r[..n] < prefix[..n]);
        let relation = catalog
            .relations
            .get(name)
            .ok_or_else(|| fail("spawn.missing_relation"))?
            .clone();
        for record in &rows[first..] {
            if record[..n] != prefix[..n] {
                break;
            }
            let ordinal = u64::from_be_bytes(record[16..].try_into().expect("ordinal"));
            catalog.with_row(&relation, ordinal, &mut f)?;
        }
        Ok(())
    }
    pub fn alternatives(
        &self,
        catalog: &mut VerifiedCatalog,
        kind: &str,
        guid: u32,
        mut f: impl FnMut(u64, u32) -> Result<()>,
    ) -> Result<()> {
        check(matches!(kind, "creature" | "gameobject"), "spawn.key")?;
        let table = format!("{kind}_spawn_entry");
        let cols = columns(&self.schema, &table)?;
        let g = position(&cols, "guid")?;
        let e = position(&cols, "entry")?;
        catalog.scan(&table, 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            if uint(cells[g])? == guid {
                f(row.ordinal, uint(cells[e])?)?;
            }
            Ok(())
        })
    }
    pub fn model_closure(&self, catalog: &mut VerifiedCatalog, entry: u32) -> Result<Value> {
        let cols = columns(&self.schema, "creature_template")?;
        let model_cols: Vec<_> = (1..=4)
            .map(|i| position(&cols, &format!("DisplayId{i}")))
            .collect::<Result<_>>()?;
        let mut pending = Vec::new();
        visit_key(catalog, "creature_template", entry, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            for &i in &model_cols {
                pending.push(uint(cells[i])?);
            }
            Ok(())
        })?;
        let cols = columns(&self.schema, "creature_model_info")?;
        let other = position(&cols, "modelid_other_gender")?;
        let alt = position(&cols, "modelid_alternative")?;
        let mut seen = BTreeSet::new();
        let mut missing = BTreeSet::new();
        while let Some(model) = pending.pop() {
            if model == 0 || !seen.insert(model) {
                continue;
            }
            check(
                seen.len() as u64
                    <= self.report["counts"]["creature_model_info"]
                        .as_u64()
                        .expect("count")
                        + 4,
                "spawn.closure_limit",
            )?;
            let mut found = false;
            visit_key(catalog, "creature_model_info", model, |row| {
                found = true;
                let mut cells = [None; 256];
                fill(&row, &mut cells)?;
                pending.push(uint(cells[other])?);
                pending.push(uint(cells[alt])?);
                Ok(())
            })?;
            if !found {
                missing.insert(model);
            }
        }
        Ok(json!({"models":seen,"missing":missing}))
    }
    fn expected_join(&self, catalog: &mut VerifiedCatalog, name: &str) -> Result<Vec<[u8; 32]>> {
        let (projection, joins) = if name == "creature" {
            (CREATURE, CJOINS)
        } else {
            (GO, GJOINS)
        };
        let index_bytes: usize = self.indexes.values().map(|v| v.capacity() * 24).sum();
        let mut charge = index_bytes as u64 + 2_097_152;
        for &(table, _, _) in joins {
            let relation = catalog
                .relations
                .get(table)
                .ok_or_else(|| fail("spawn.missing_relation"))?;
            charge = add(charge, relation.rows * 256)?;
        }
        check(charge <= DECODED as u64, "spawn.join_limit")?;
        let mut ancillary = Vec::new();
        for &(table, key, value) in joins {
            let cols = columns(&self.schema, table)?;
            let k = position(&cols, key)?;
            let v = position(&cols, value)?;
            let mut index = BTreeMap::<u32, Vec<NumericCell>>::new();
            catalog.scan(table, 0, 256, |row| {
                let mut cells = [None; 256];
                fill(&row, &mut cells)?;
                if cells[k].is_some() {
                    index
                        .entry(uint(cells[k])?)
                        .or_default()
                        .push(NumericCell::new(cells[v])?);
                }
                Ok(())
            })?;
            ancillary.push(index);
        }
        let cols = columns(&self.schema, name)?;
        let positions: Vec<_> = projection
            .iter()
            .map(|p| position(&cols, p))
            .collect::<Result<_>>()?;
        let guid = position(&cols, "guid")?;
        let entry = position(&cols, "id")?;
        let null = [NumericCell::new(None)?];
        let mut hashes = Vec::new();
        catalog.scan(name, 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let mut projected = [None; 20];
            for (i, &p) in positions.iter().enumerate() {
                projected[i] = cells[p];
            }
            let g = uint(cells[guid])?;
            let e = uint(cells[entry])?;
            let mut choices: [&[NumericCell]; 4] = [&null; 4];
            for (i, index) in ancillary.iter().enumerate() {
                let key = if joins[i].1 == "id" { e } else { g };
                choices[i] = index.get(&key).map_or(&null, Vec::as_slice);
            }
            let mut offsets = [0usize; 4];
            loop {
                for i in 0..joins.len() {
                    projected[positions.len() + i] = choices[i][offsets[i]].raw();
                }
                let next_capacity = if hashes.len() == hashes.capacity() {
                    (hashes.capacity() * 2).max(4)
                } else {
                    hashes.capacity()
                };
                check(
                    hashes.len() < 2_000_000
                        && charge + (next_capacity as u64) * 64 <= DECODED as u64,
                    "spawn.join_limit",
                )?;
                hashes.push(hash_cells(&projected[..positions.len() + joins.len()]));
                let mut i = joins.len();
                while i > 0 {
                    i -= 1;
                    offsets[i] += 1;
                    if offsets[i] < choices[i].len() {
                        break;
                    }
                    offsets[i] = 0;
                }
                if i == 0 && offsets[0] == 0 {
                    break;
                }
            }
            Ok(())
        })?;
        hashes.sort_unstable();
        Ok(hashes)
    }
    pub fn verify_indexes(&mut self, catalog: &mut VerifiedCatalog) -> Result<()> {
        let raw = catalog
            .store
            .named("spawn-indexes.json", CHUNK, &mut catalog.metrics, CACHE)?;
        let value = bounded_json(&raw)?;
        catalog_schema::fields(
            &value,
            &["format", "catalog_sha256", "indexes", "complete"],
            "spawn.index",
        )?;
        check(
            value["format"] == "benilla-spawn-indexes-v3"
                && value["catalog_sha256"] == catalog.manifest["catalog_sha256"]
                && value["complete"] == false,
            "spawn.index",
        )?;
        catalog_schema::fields(
            &value["indexes"],
            &["creature", "gameobject"],
            "spawn.index",
        )?;
        for (name, records) in &self.indexes {
            let root: Root = serde_json::from_value(value["indexes"][name].clone())
                .map_err(|_| fail("spawn.index"))?;
            root_descriptor(&root, records.len() as u64)?;
            let sid = catalog
                .relations
                .get(name)
                .ok_or_else(|| fail("spawn.missing_relation"))?
                .id;
            let mut at = 0usize;
            let _ =
                catalog.walk_keys(&root, sid, 0, false, &mut None, &mut |_, key, ordinal| {
                    let record = records.get(at).ok_or_else(|| fail("spawn.index"))?;
                    check(
                        key == &record[..16]
                            && ordinal
                                == u64::from_be_bytes(record[16..].try_into().expect("ordinal")),
                        "spawn.index",
                    )?;
                    at += 1;
                    Ok(())
                })?;
            check(at == records.len(), "spawn.index")?;
        }
        self.report["index_pages"] = value["indexes"].clone();
        Ok(())
    }
    pub fn verify_occurrences(&mut self, catalog: &mut VerifiedCatalog, path: &Path) -> Result<()> {
        let metadata = reader::Held::open(&path.join("occurrences.json"), CHUNK as u64)?;
        let raw = metadata.read(0, metadata.len() as usize, &mut Metrics::default(), CACHE)?;
        let meta = bounded_json(&raw)?;
        catalog_schema::fields(
            &meta,
            &["format", "streams", "source_snapshot", "complete"],
            "spawn.occurrences",
        )?;
        check(
            meta["format"] == "benilla-spawn-occurrences-v3"
                && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
                && meta["complete"] == false,
            "spawn.occurrences",
        )?;
        catalog_schema::fields(
            &meta["streams"],
            &["creature", "gameobject"],
            "spawn.occurrences",
        )?;
        let mut buffer = vec![0; FRAME];
        for name in ["creature", "gameobject"] {
            let expected = self.expected_join(catalog, name)?;
            let d = &meta["streams"][name];
            catalog_schema::fields(d, &["sha256", "bytes", "rows", "query"], "spawn.query")?;
            let id = if name == "creature" {
                "Globals/ObjectMgr:1877"
            } else {
                "Globals/ObjectMgr:2086"
            };
            check(
                d["query"] == catalog_schema::POLICY["loaders"][id]["query"],
                "spawn.query",
            )?;
            let held = reader::Held::open(&path.join(format!("{name}.rows")), MAX_RAW)?;
            let columns = if name == "creature" {
                CREATURE.len() + CJOINS.len()
            } else {
                GO.len() + GJOINS.len()
            };
            self.report["joined"][name] = verify_stream(&held, columns, &expected, d, &mut buffer)?;
        }
        Ok(())
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
}
fn verify_stream(
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
        check(at + 4 <= held.len(), "spawn.exhausted")?;
        held.read_into(at, &mut buffer[..4])?;
        let size = le(buffer, 0, 4)? as usize;
        check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
        held.read_into(at + 4, &mut buffer[4..size])?;
        let row = RowView::new(&buffer[..size], ordinal as u64, columns)?;
        row.validate()?;
        let mut cells = [None; 256];
        fill(&row, &mut cells)?;
        seen.push(hash_cells(&cells[..columns]));
        hash.update(&buffer[..size]);
        at += size as u64;
    }
    check(at == held.len(), "spawn.trailing")?;
    let occurrence_hash = hex(&hash.finalize());
    check(
        d["bytes"] == held.len()
            && d["rows"] == seen.len() as u64
            && d["sha256"] == occurrence_hash,
        "spawn.hash",
    )?;
    seen.sort_unstable();
    check(seen == expected, "spawn.join_mismatch")?;
    let mut multiset = Sha256::new();
    for h in &seen {
        multiset.update(h);
    }
    Ok(
        json!({"rows":seen.len(),"occurrence_sha256":occurrence_hash,"multiset_sha256":hex(&multiset.finalize())}),
    )
}
pub(super) fn visit_key(
    catalog: &mut VerifiedCatalog,
    table: &str,
    value: u32,
    mut f: impl FnMut(RowView<'_>) -> Result<()>,
) -> Result<()> {
    let mut digits = [0u8; 10];
    let mut start = digits.len();
    let mut remaining = value;
    loop {
        start -= 1;
        digits[start] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    let text = &digits[start..];
    let mut key = [0u8; 14];
    key[..4].copy_from_slice(&(text.len() as u32).to_le_bytes());
    key[4..4 + text.len()].copy_from_slice(text);
    let mut ordinals = Vec::new();
    catalog.lookup(table, &key[..4 + text.len()], |ordinal| {
        check(ordinals.len() < DECODED / 16, "spawn.closure_limit")?;
        ordinals.push(ordinal);
        Ok(())
    })?;
    let relation = catalog
        .relations
        .get(table)
        .ok_or_else(|| fail("spawn.missing_relation"))?
        .clone();
    for ordinal in ordinals {
        catalog.with_row(&relation, ordinal, &mut f)?;
    }
    Ok(())
}
pub fn diagnose_catalog_spawns(root: &Path, occurrences: &Path) -> Result<Value> {
    let mut catalog = VerifiedCatalog::open(root, false)?;
    prepare(&mut catalog, occurrences)
}
pub(super) fn prepare(catalog: &mut VerifiedCatalog, occurrences: &Path) -> Result<Value> {
    let mut views = SpawnViews::build(catalog)?;
    views.verify_indexes(catalog)?;
    views.verify_occurrences(catalog, occurrences)?;
    views.report["view_sha256"] = json!(json::digest(&views.report));
    Ok(views.report)
}

#[cfg(test)]
#[path = "spawns_tests.rs"]
mod tests;
