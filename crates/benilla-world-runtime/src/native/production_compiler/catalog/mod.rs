// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable catalog-v3 access. Successful verification never grants admission.
use super::{catalog_schema, check, fail, json, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::Instant,
};
mod reader;
mod spawns;
pub use spawns::{diagnose_catalog_spawns, SpawnViews};
mod groups;
pub use groups::diagnose_catalog_groups;
pub(crate) use groups::{LinkingCatalogRows, SpawnGroupCatalogRows};
mod pools;
pub use pools::diagnose_catalog_pools;
pub(crate) use pools::PoolCatalogRows;
mod events;
pub use events::diagnose_catalog_events;
mod dbc;
pub use dbc::transport_inputs::{
    TransportAnimationRecord, TransportGameObject, TransportIntegrityRow, TransportSqlRow,
};
pub use dbc::{diagnose_catalog_dbc, DbcView as CatalogDbcView};
mod scripts;
pub use scripts::diagnose_catalog_scripts;
mod spells;
pub use spells::diagnose_catalog_spells;
mod content;
pub use content::diagnose_catalog_content;
mod preparation;
pub use preparation::diagnose_catalog_preparation;
#[cfg(test)]
mod tests;
pub(crate) use reader::Held;
use reader::Store;
const PAGE: usize = 4096;
const CHUNK: usize = 1_048_576;
const FRAME: usize = 65_536;
const CACHE: usize = 8_388_608;
const DECODED: usize = 33_554_432;
const MAX_INDEX: usize = 134_217_728;
const MAX_RAW: u64 = 1_073_741_824;
const MAGIC: &[u8; 8] = b"BNCAT3\0\0";
const INDEX_MAGIC: &[u8; 8] = b"BNCIDX3\0";

type KeyPair = (Vec<u8>, u64);
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    sha256: String,
    bytes: u64,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Root {
    sha256: String,
    bytes: u64,
    count: u64,
    height: u64,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Relation {
    id: u32,
    columns: usize,
    key_columns: Vec<usize>,
    rows: u64,
    ordinal: Root,
    key: Root,
}
#[derive(Default)]
struct Metrics {
    reads: u64,
    bytes: u64,
    max_read: usize,
    max_frame: usize,
    decoded: usize,
    cache_peak: usize,
    depth: u64,
}

fn number(v: &Value) -> Result<u64> {
    v.as_u64().ok_or_else(|| fail("catalog.overflow"))
}
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| fail("catalog.overflow"))
}
fn le(data: &[u8], at: usize, n: usize) -> Result<u64> {
    let slice = data
        .get(at..at.checked_add(n).ok_or_else(|| fail("catalog.overflow"))?)
        .ok_or_else(|| fail("catalog.truncated"))?;
    Ok(slice
        .iter()
        .enumerate()
        .fold(0, |v, (i, b)| v | (u64::from(*b) << (i * 8))))
}
fn bounded_json(raw: &[u8]) -> Result<Value> {
    let value = json::decode_with_limits(raw, CHUNK, 32, 100_000).map_err(|e| {
        if matches!(e.id.as_str(), "json.nesting_limit" | "json.node_limit") {
            fail("catalog.json_limit")
        } else {
            e
        }
    })?;
    check(json::canonical(&value) == raw, "catalog.json")?;
    Ok(value)
}
fn descriptor(v: &Value) -> Result<Descriptor> {
    catalog_schema::fields(v, &["sha256", "bytes"], "catalog.descriptor")?;
    let d = Descriptor {
        sha256: v["sha256"]
            .as_str()
            .ok_or_else(|| fail("catalog.path"))?
            .to_owned(),
        bytes: number(&v["bytes"])?,
    };
    validate_descriptor(&d, CHUNK as u64)?;
    Ok(d)
}
fn validate_descriptor(d: &Descriptor, limit: u64) -> Result<()> {
    check(
        d.sha256.len() == 64
            && d.sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "catalog.path",
    )?;
    check(d.bytes <= limit, "catalog.overflow")
}
fn root_descriptor(root: &Root, count: u64) -> Result<()> {
    validate_descriptor(
        &Descriptor {
            sha256: root.sha256.clone(),
            bytes: root.bytes,
        },
        PAGE as u64,
    )?;
    check(
        root.bytes == PAGE as u64 && root.count == count,
        "catalog.index",
    )?;
    check(root.height < 8, "catalog.depth")
}
fn limits() -> Value {
    json!({"manifest":CHUNK,"chunk":CHUNK,"frame":FRAME,"page":PAGE,"batch_rows":256,"batch_bytes":CHUNK,"cache":CACHE,"decoded_batch":DECODED,"rows":2_000_000,"index":MAX_INDEX,"raw":MAX_RAW,"depth":8,"relations":512,"columns":8192,"dbc_total":134_217_728,"dbc_file":67_108_864})
}

type CachedChunk = (String, Arc<[u8]>, Vec<(usize, usize)>);
/// Owns checked file handles and a shared bounded raw cache, not decoded catalogs.
pub struct VerifiedCatalog {
    manifest: Value,
    relations: BTreeMap<String, Relation>,
    store: Store,
    metrics: Metrics,
    seen: HashSet<(u8, String)>,
    pages: HashMap<String, (Arc<[u8]>, u64)>,
    clock: u64,
    cached: Option<CachedChunk>,
    rows_hash: String,
    total_rows: u64,
    columns: u64,
    dbc_count: usize,
    dbc_bytes: u64,
    seconds: f64,
    cache_limit: usize,
}
impl VerifiedCatalog {
    pub(crate) fn execution_identity(&self) -> &Value {
        &self.manifest
    }
    pub(crate) fn execution_linking_rows(
        &mut self,
        occurrences: &Path,
    ) -> Result<LinkingCatalogRows> {
        LinkingCatalogRows::open(self, occurrences)
    }
    pub(crate) fn execution_spawn_group_rows(
        &mut self,
        occurrences: &Path,
    ) -> Result<SpawnGroupCatalogRows> {
        SpawnGroupCatalogRows::open(self, occurrences)
    }
    /// SQL-equivalent raw-world query. Only the sole GUID is read by the loader;
    /// two matches reject before Fetch, so their relative order is unconsumed.
    pub(crate) fn execution_linking_unique_master(
        &mut self,
        entry: u32,
        map: u32,
    ) -> Result<Vec<u32>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, "creature")?;
        let positions = [
            spawns::position(&columns, "guid")?,
            spawns::position(&columns, "id")?,
            spawns::position(&columns, "map")?,
        ];
        let relation = self
            .relations
            .get("creature")
            .ok_or_else(|| fail("catalog.table"))?
            .clone();
        let mut guids = Vec::new();
        for ordinal in 0..relation.rows {
            self.with_row(&relation, ordinal, &mut |row| {
                let mut cells = [None; 256];
                spawns::fill(&row, &mut cells)?;
                if spawns::uint(cells[positions[1]])? == entry
                    && spawns::uint(cells[positions[2]])? == map
                {
                    guids.push(spawns::uint(cells[positions[0]])?);
                }
                Ok(())
            })?;
            if guids.len() == 2 {
                break;
            }
        }
        Ok(guids)
    }
    pub(crate) fn execution_row(
        &mut self,
        name: &str,
        ordinal: u64,
        mut visit: impl FnMut(RowView<'_>) -> Result<()>,
    ) -> Result<()> {
        let relation = self
            .relations
            .get(name)
            .ok_or_else(|| fail("catalog.table"))?
            .clone();
        self.with_row(&relation, ordinal, &mut visit)
    }
    pub(crate) fn execution_spawn_row(
        &mut self,
        name: &str,
        guid: u32,
        ordinal: u64,
    ) -> Result<RetainedCatalogRow> {
        check(matches!(name, "creature" | "gameobject"), "catalog.table")?;
        let mut retained = None;
        spawns::visit_key(self, name, guid, |row| {
            if row.ordinal == ordinal {
                retained = Some(RetainedCatalogRow {
                    frame: row.frame.to_vec(),
                    ordinal,
                    columns: row.columns,
                    bitmap: row.bitmap,
                });
            }
            Ok(())
        })?;
        retained.ok_or_else(|| fail("catalog.spawn_occurrence_binding"))
    }
    /// Narrow raw loader operands. This does not certify ObjectMgr acceptance.
    pub(crate) fn execution_spawn_location(
        &mut self,
        name: &str,
        guid: u32,
        ordinal: u64,
    ) -> Result<(u32, f32, f32)> {
        let retained = self.execution_spawn_row(name, guid, ordinal)?;
        let d = descriptor(&self.manifest["schema_root"])?;
        let schema = bounded_json(&self.object(&d, 0)?)?;
        let columns = spawns::columns(&schema, name)?;
        let positions = [
            spawns::position(&columns, "map")?,
            spawns::position(&columns, "position_x")?,
            spawns::position(&columns, "position_y")?,
        ];
        let mut cells = [None; 256];
        spawns::fill(&retained.view(), &mut cells)?;
        Ok((
            spawns::uint(cells[positions[0]])?,
            spawns::floating(cells[positions[1]])?,
            spawns::floating(cells[positions[2]])?,
        ))
    }
    pub(crate) fn execution_spawn_ordinal(&mut self, name: &str, guid: u32) -> Result<u64> {
        check(matches!(name, "creature" | "gameobject"), "catalog.table")?;
        let mut ordinal = None;
        spawns::visit_key(self, name, guid, |row| {
            check(ordinal.is_none(), "catalog.spawn_duplicate_guid")?;
            ordinal = Some(row.ordinal);
            Ok(())
        })?;
        ordinal.ok_or_else(|| fail("catalog.spawn_occurrence_binding"))
    }
    /// Candidate mode validates unpublished bytes with exactly the production reader.
    pub fn open(path: &Path, candidate: bool) -> Result<Self> {
        Self::open_with_cache(path, candidate, CACHE)
    }
    pub fn open_with_cache(path: &Path, candidate: bool, cache_limit: usize) -> Result<Self> {
        Self::open_policy(path, candidate, cache_limit, &catalog_schema::POLICY)
    }
    fn open_policy(
        path: &Path,
        candidate: bool,
        cache_limit: usize,
        policy: &Value,
    ) -> Result<Self> {
        let started = Instant::now();
        let mut store = Store::open(path)?;
        let mut metrics = Metrics::default();
        let raw = store.named(
            if candidate {
                "candidate.json"
            } else {
                "manifest.json"
            },
            CHUNK,
            &mut metrics,
            cache_limit,
        )?;
        let manifest = bounded_json(&raw)?;
        catalog_schema::fields(
            &manifest,
            &[
                "format",
                "core_revision",
                "database_revision",
                "source_snapshot",
                "schema_root",
                "loader_coverage_root",
                "relation_root",
                "index_root",
                "dbc_root",
                "registration_root",
                "occurrence_root",
                "scope",
                "limits_policy",
                "completeness",
                "catalog_sha256",
            ],
            "catalog.manifest",
        )?;
        check(
            manifest["format"] == "benilla-classic-catalog-v3"
                && manifest["core_revision"] == catalog_schema::POLICY["core_revision"]
                && manifest["database_revision"] == catalog_schema::POLICY["database_revision"],
            "catalog.version",
        )?;
        check(
            manifest["scope"] == json!({"primary_maps":[0,1],"global":true,"other_maps":true}),
            "catalog.scope",
        )?;
        check(
            manifest["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
                && manifest["completeness"] == false
                && manifest["registration_root"].is_null()
                && manifest["occurrence_root"].is_null(),
            "catalog.incomplete",
        )?;
        check(manifest["limits_policy"] == limits(), "catalog.limits")?;
        let mut identity = manifest.clone();
        identity
            .as_object_mut()
            .expect("manifest object")
            .remove("catalog_sha256");
        check(
            manifest["catalog_sha256"] == json::digest(&identity),
            "catalog.hash",
        )?;
        let mut this = Self {
            manifest,
            relations: BTreeMap::new(),
            store,
            metrics,
            seen: HashSet::new(),
            pages: HashMap::new(),
            clock: 0,
            cached: None,
            rows_hash: String::new(),
            total_rows: 0,
            columns: 0,
            dbc_count: 0,
            dbc_bytes: 0,
            seconds: 0.0,
            cache_limit,
        };
        let schema = bounded_json(&this.object(&descriptor(&this.manifest["schema_root"])?, 0)?)?;
        let coverage =
            bounded_json(&this.object(&descriptor(&this.manifest["loader_coverage_root"])?, 0)?)?;
        let schema_report = catalog_schema::verify_with_policy(&schema, &coverage, policy)?;
        this.columns = number(&schema_report["columns"])?;
        let relations =
            bounded_json(&this.object(&descriptor(&this.manifest["relation_root"])?, 0)?)?;
        let map = relations.as_object().ok_or_else(|| fail("catalog.table"))?;
        check(
            map.keys()
                .eq(schema["tables"].as_object().expect("schema tables").keys()),
            "catalog.table",
        )?;
        let roots: BTreeMap<_, _> = map
            .iter()
            .map(|(n, r)| (n, json!({"ordinal":r["ordinal"],"key":r["key"]})))
            .collect();
        check(
            this.manifest["index_root"] == json::digest(&roots),
            "catalog.hash",
        )?;
        for (sid, (name, value)) in map.iter().enumerate() {
            catalog_schema::fields(
                value,
                &["id", "columns", "key_columns", "rows", "ordinal", "key"],
                "catalog.relation",
            )?;
            number(&value["rows"])?;
            for member in ["ordinal", "key"] {
                let root = &value[member];
                catalog_schema::fields(
                    root,
                    &["sha256", "bytes", "count", "height"],
                    "catalog.descriptor",
                )?;
                descriptor(&json!({"sha256":root["sha256"],"bytes":root["bytes"]}))?;
                number(&root["count"])?;
                number(&root["height"])?;
            }
            check(
                value["key_columns"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(|v| v.as_u64().is_some())),
                "catalog.index",
            )?;
            let table = &schema["tables"][name];
            let columns = table["columns"].as_array().expect("schema columns");
            check(
                number(&value["id"])? == (sid + 1) as u64
                    && number(&value["columns"])? == columns.len() as u64,
                "catalog.schema_mismatch",
            )?;
            let r: Relation =
                serde_json::from_value(value.clone()).map_err(|_| fail("catalog.relation"))?;
            check(
                r.id as usize == sid + 1 && r.columns == columns.len(),
                "catalog.schema_mismatch",
            )?;
            let mut keys: Vec<_> = table["indexes"]
                .as_array()
                .expect("schema indexes")
                .iter()
                .filter(|i| i["INDEX_NAME"] == "PRIMARY")
                .collect();
            keys.sort_by_key(|i| i["SEQ_IN_INDEX"].as_u64());
            let expected: Vec<_> = if keys.is_empty() {
                (0..columns.len()).collect()
            } else {
                keys.iter()
                    .map(|k| {
                        columns
                            .iter()
                            .position(|c| c["COLUMN_NAME"] == k["COLUMN_NAME"])
                            .expect("schema key column")
                    })
                    .collect()
            };
            check(r.key_columns == expected, "catalog.index")?;
            this.total_rows = add(this.total_rows, r.rows)?;
            check(this.total_rows <= 2_000_000, "catalog.row_limit")?;
            root_descriptor(&r.ordinal, r.rows)?;
            root_descriptor(&r.key, r.rows)?;
            this.relations.insert(name.clone(), r);
        }
        drop(schema);
        drop(coverage);
        drop(relations);
        let mut rows_hash = Sha256::new();
        rows_hash.update(b"benilla-catalog-rows-v3\0");
        let names: Vec<_> = this.relations.keys().cloned().collect();
        for name in names {
            let r = this.relations[&name].clone();
            let mut chunk_number = 0;
            this.walk_ordinal(
                &r.ordinal,
                r.id,
                0,
                true,
                &mut |s, h, start, count, size| {
                    check(count > 0 && count <= 4096, "catalog.range")?;
                    s.unique(0, h)?;
                    let data = s.object(
                        &Descriptor {
                            sha256: h.to_owned(),
                            bytes: size,
                        },
                        0,
                    )?;
                    let offsets = s.frames(&data, &r, start, count, Some(chunk_number))?;
                    for (i, (a, b)) in offsets.into_iter().enumerate() {
                        rows_hash.update(r.id.to_le_bytes());
                        rows_hash.update((start + i as u64).to_le_bytes());
                        rows_hash.update(((b - a) as u32).to_le_bytes());
                        rows_hash.update(&data[a..b]);
                    }
                    chunk_number += 1;
                    Ok(())
                },
            )?;
            let mut last = None;
            this.walk_keys(&r.key, r.id, 0, true, &mut last, &mut |s, key, ordinal| {
                check(ordinal < r.rows, "catalog.index")?;
                s.with_row(&r, ordinal, &mut |row| {
                    check(row.key_matches(&r.key_columns, key)?, "catalog.index")
                })
            })?;
        }
        let dbc = bounded_json(&this.object(&descriptor(&this.manifest["dbc_root"])?, 0)?)?;
        let dbc = dbc
            .as_object()
            .ok_or_else(|| fail("catalog.dbc_inventory"))?;
        for (name, v) in dbc {
            check(
                name.ends_with(".dbc") && !name.contains('/') && !name.contains('\\'),
                "catalog.path",
            )?;
            catalog_schema::fields(v, &["sha256", "bytes"], "catalog.descriptor")?;
            let d = Descriptor {
                sha256: v["sha256"]
                    .as_str()
                    .ok_or_else(|| fail("catalog.path"))?
                    .to_owned(),
                bytes: number(&v["bytes"])?,
            };
            validate_descriptor(&d, 67_108_864)?;
            this.dbc_bytes = add(this.dbc_bytes, d.bytes)?;
            check(this.dbc_bytes <= 134_217_728, "catalog.raw_limit")?;
            this.store.blob(&d, &mut this.metrics, this.cache_limit)?;
        }
        this.dbc_count = dbc.len();
        this.rows_hash = format!("{:x}", rows_hash.finalize());
        this.seconds = started.elapsed().as_secs_f64();
        this.store.check_all()?;
        Ok(this)
    }
    fn object(&mut self, d: &Descriptor, namespace: u8) -> Result<Arc<[u8]>> {
        validate_descriptor(d, CHUNK as u64)?;
        self.clock += 1;
        if namespace == 1 {
            if let Some((data, clock)) = self.pages.get_mut(&d.sha256) {
                self.store.check(namespace, &d.sha256)?;
                *clock = self.clock;
                return Ok(Arc::clone(data));
            }
        }
        let data: Arc<[u8]> = self
            .store
            .object(d, namespace, &mut self.metrics, self.cache_limit)?
            .into();
        if namespace == 1 {
            if self.pages.len() == 256 {
                let oldest = self
                    .pages
                    .iter()
                    .min_by_key(|(_, (_, t))| *t)
                    .map(|(h, _)| h.clone())
                    .expect("full cache");
                self.pages.remove(&oldest);
            }
            self.pages
                .insert(d.sha256.clone(), (Arc::clone(&data), self.clock));
        }
        let used = self.pages.len() * PAGE
            + data.len()
            + self.cached.as_ref().map_or(0, |(_, d, _)| d.len())
            + PAGE * 8
            + FRAME;
        self.metrics.cache_peak = self.metrics.cache_peak.max(used);
        check(used <= self.cache_limit, "catalog.cache_limit")?;
        Ok(data)
    }
    fn unique(&mut self, namespace: u8, h: &str) -> Result<()> {
        check(
            self.seen.insert((namespace, h.to_owned())),
            "catalog.duplicate_descriptor",
        )?;
        check(
            self.seen.len() <= MAX_INDEX / PAGE + 2_000_000,
            "catalog.index_limit",
        )
    }
    fn page(
        &mut self,
        h: &str,
        sid: u32,
        kind: u64,
        unique: bool,
    ) -> Result<(Arc<[u8]>, u64, u64)> {
        if unique {
            self.unique(1, h)?;
        }
        let data = self.object(
            &Descriptor {
                sha256: h.to_owned(),
                bytes: PAGE as u64,
            },
            1,
        )?;
        check(
            &data[..8] == INDEX_MAGIC
                && le(&data, 8, 2)? == 3
                && le(&data, 10, 2)? == kind
                && le(&data, 12, 4)? == u64::from(sid),
            "catalog.index",
        )?;
        let first = le(&data, 16, 8)?;
        let count = le(&data, 24, 8)?;
        let width = match kind {
            1 | 2 => 48,
            3 => 80,
            4 => 120,
            _ => return Err(fail("catalog.index")),
        };
        check(count <= ((PAGE - 32) / width) as u64, "catalog.index")?;
        check(
            data[32 + count as usize * width..].iter().all(|b| *b == 0),
            "catalog.trailing",
        )?;
        Ok((data, first, count))
    }
    fn walk_ordinal(
        &mut self,
        root: &Root,
        sid: u32,
        start: u64,
        unique: bool,
        f: &mut impl FnMut(&mut Self, &str, u64, u64, u64) -> Result<()>,
    ) -> Result<()> {
        check(root.height < 8, "catalog.depth")?;
        self.metrics.depth = self.metrics.depth.max(root.height + 1);
        let (data, first, count) = self.page(
            &root.sha256,
            sid,
            if root.height == 0 { 1 } else { 2 },
            unique,
        )?;
        check(first == start, "catalog.range")?;
        let mut end = start;
        for i in 0..count as usize {
            let at = 32 + i * 48;
            let h = hex(&data[at..at + 32]);
            let s = le(&data, at + 32, 8)?;
            let n = le(&data, at + 40, 4)?;
            let b = le(&data, at + 44, 4)?;
            check(s == end && n > 0, "catalog.range")?;
            if root.height == 0 {
                check(b <= CHUNK as u64, "catalog.overflow")?;
                f(self, &h, s, n, b)?;
            } else {
                check(b == PAGE as u64, "catalog.index")?;
                self.walk_ordinal(
                    &Root {
                        sha256: h,
                        bytes: b,
                        count: n,
                        height: root.height - 1,
                    },
                    sid,
                    s,
                    unique,
                    f,
                )?;
            }
            end = add(s, n)?;
        }
        check(end - start == root.count, "catalog.range")
    }
    fn key_record(&mut self, data: &[u8], at: usize) -> Result<KeyPair> {
        let size = le(data, at, 4)? as usize;
        check(size <= FRAME, "catalog.long_key")?;
        check(le(data, at + 4, 4)? == 0, "catalog.index")?;
        let ordinal = le(data, at + 8, 8)?;
        let raw = data
            .get(at + 16..at + 80)
            .ok_or_else(|| fail("catalog.truncated"))?;
        let key = if size <= 64 {
            check(raw[size..].iter().all(|b| *b == 0), "catalog.trailing")?;
            raw[..size].to_vec()
        } else {
            check(raw[32..].iter().all(|b| *b == 0), "catalog.trailing")?;
            self.object(
                &Descriptor {
                    sha256: hex(&raw[..32]),
                    bytes: size as u64,
                },
                0,
            )?
            .to_vec()
        };
        Ok((key, ordinal))
    }
    fn walk_keys(
        &mut self,
        root: &Root,
        sid: u32,
        depth: u64,
        unique: bool,
        last: &mut Option<KeyPair>,
        f: &mut impl FnMut(&mut Self, &[u8], u64) -> Result<()>,
    ) -> Result<Option<KeyPair>> {
        check(depth < 8 && root.height < 8, "catalog.depth")?;
        self.metrics.depth = self.metrics.depth.max(depth + 1);
        let (data, first, count) = self.page(
            &root.sha256,
            sid,
            if root.height == 0 { 3 } else { 4 },
            unique,
        )?;
        check(first == 0, "catalog.index")?;
        let width = if root.height == 0 { 80 } else { 120 };
        let mut total = 0;
        let mut final_pair = None;
        for i in 0..count as usize {
            let at = 32 + i * width;
            let pair = self.key_record(&data, at)?;
            if root.height == 0 {
                check(
                    last.as_ref().is_none_or(|p| p < &pair),
                    "catalog.duplicate_row",
                )?;
                f(self, &pair.0, pair.1)?;
                total = add(total, 1)?;
                *last = Some(pair.clone());
            } else {
                let h = hex(&data[at + 80..at + 112]);
                let n = le(&data, at + 112, 8)?;
                let final_child = self.walk_keys(
                    &Root {
                        sha256: h,
                        bytes: PAGE as u64,
                        count: n,
                        height: root.height - 1,
                    },
                    sid,
                    depth + 1,
                    unique,
                    last,
                    f,
                )?;
                check(final_child.as_ref() == Some(&pair), "catalog.index")?;
                total = add(total, n)?;
            }
            final_pair = Some(pair);
        }
        check(total == root.count, "catalog.index")?;
        Ok(final_pair)
    }
    fn frames(
        &mut self,
        data: &[u8],
        r: &Relation,
        start: u64,
        count: u64,
        chunk_number: Option<u64>,
    ) -> Result<Vec<(usize, usize)>> {
        check(data.len() >= 32, "catalog.truncated")?;
        check(
            &data[..8] == MAGIC
                && le(data, 8, 2)? == 3
                && le(data, 10, 2)? == 1
                && le(data, 12, 4)? == u64::from(r.id)
                && le(data, 24, 4)? == count
                && chunk_number.is_none_or(|c| le(data, 16, 8).ok() == Some(c)),
            "catalog.frame",
        )?;
        let payload = le(data, 28, 4)? as usize;
        check(payload <= data.len() - 32, "catalog.truncated")?;
        check(payload == data.len() - 32, "catalog.trailing")?;
        check(count <= 4096, "catalog.range")?;
        let mut at = 32;
        let mut offsets = Vec::with_capacity(count as usize);
        for ordinal in start..add(start, count)? {
            let size = le(data, at, 4)? as usize;
            check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
            let end = at
                .checked_add(size)
                .ok_or_else(|| fail("catalog.overflow"))?;
            let frame = data.get(at..end).ok_or_else(|| fail("catalog.truncated"))?;
            let row = RowView::new(frame, ordinal, r.columns)?;
            row.validate()?;
            self.metrics.max_frame = self.metrics.max_frame.max(size);
            self.metrics.decoded = self.metrics.decoded.max(std::mem::size_of::<RowView<'_>>());
            offsets.push((at, end));
            at = end;
        }
        check(at == data.len(), "catalog.trailing")?;
        Ok(offsets)
    }
    fn locate(&mut self, root: &Root, sid: u32, ordinal: u64) -> Result<(String, u64, u64, u64)> {
        let (data, _, count) = self.page(
            &root.sha256,
            sid,
            if root.height == 0 { 1 } else { 2 },
            false,
        )?;
        for i in 0..count as usize {
            let at = 32 + i * 48;
            let s = le(&data, at + 32, 8)?;
            let n = le(&data, at + 40, 4)?;
            if s <= ordinal && ordinal < add(s, n)? {
                let h = hex(&data[at..at + 32]);
                let b = le(&data, at + 44, 4)?;
                if root.height == 0 {
                    return Ok((h, s, n, b));
                }
                return self.locate(
                    &Root {
                        sha256: h,
                        bytes: b,
                        count: n,
                        height: root.height - 1,
                    },
                    sid,
                    ordinal,
                );
            }
        }
        Err(fail("catalog.index"))
    }
    fn with_row(
        &mut self,
        r: &Relation,
        ordinal: u64,
        f: &mut impl FnMut(RowView<'_>) -> Result<()>,
    ) -> Result<()> {
        check(ordinal < r.rows, "catalog.range")?;
        let (h, start, count, size) = self.locate(&r.ordinal, r.id, ordinal)?;
        if self.cached.as_ref().is_none_or(|(old, _, _)| old != &h) {
            self.cached = None;
            let data = self.object(
                &Descriptor {
                    sha256: h.clone(),
                    bytes: size,
                },
                0,
            )?;
            let offsets = self.frames(&data, r, start, count, None)?;
            self.cached = Some((h.clone(), data, offsets));
        } else {
            self.store.check(0, &h)?;
        }
        let (_, data, offsets) = self.cached.as_ref().expect("filled chunk");
        let (a, b) = offsets[(ordinal - start) as usize];
        f(RowView::new(&data[a..b], ordinal, r.columns)?)
    }
    /// Visits bounded-lifetime row views in retained occurrence order.
    pub fn scan(
        &mut self,
        name: &str,
        start: u64,
        batch_rows: usize,
        mut f: impl FnMut(RowView<'_>) -> Result<()>,
    ) -> Result<()> {
        check((1..=256).contains(&batch_rows), "catalog.batch_limit")?;
        let r = self
            .relations
            .get(name)
            .ok_or_else(|| fail("catalog.table"))?
            .clone();
        check(start <= r.rows, "catalog.range")?;
        self.walk_ordinal(
            &r.ordinal,
            r.id,
            0,
            false,
            &mut |s, h, first, count, size| {
                if add(first, count)? <= start {
                    return Ok(());
                }
                let data = s.object(
                    &Descriptor {
                        sha256: h.to_owned(),
                        bytes: size,
                    },
                    0,
                )?;
                let offsets = s.frames(&data, &r, first, count, None)?;
                for (i, (a, b)) in offsets.into_iter().enumerate() {
                    let ordinal = first + i as u64;
                    if ordinal >= start {
                        f(RowView::new(&data[a..b], ordinal, r.columns)?)?;
                    }
                }
                Ok(())
            },
        )
    }
    /// An empty visit is verified absence, never an unavailable chunk.
    pub fn lookup(
        &mut self,
        name: &str,
        key: &[u8],
        mut f: impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        check(key.len() <= FRAME, "catalog.long_key")?;
        let r = self
            .relations
            .get(name)
            .ok_or_else(|| fail("catalog.table"))?
            .clone();
        self.lookup_page(&r.key, r.id, key, &mut f)
    }
    fn lookup_page(
        &mut self,
        root: &Root,
        sid: u32,
        key: &[u8],
        f: &mut impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        let (data, _, count) = self.page(
            &root.sha256,
            sid,
            if root.height == 0 { 3 } else { 4 },
            false,
        )?;
        let width = if root.height == 0 { 80 } else { 120 };
        for i in 0..count as usize {
            let at = 32 + i * width;
            let (k, ordinal) = self.key_record(&data, at)?;
            if root.height > 0 {
                if k.as_slice() >= key {
                    let h = hex(&data[at + 80..at + 112]);
                    let n = le(&data, at + 112, 8)?;
                    self.lookup_page(
                        &Root {
                            sha256: h,
                            bytes: PAGE as u64,
                            count: n,
                            height: root.height - 1,
                        },
                        sid,
                        key,
                        f,
                    )?;
                }
                if k.as_slice() > key {
                    break;
                }
            } else if k.as_slice() == key {
                f(ordinal)?;
            } else if k.as_slice() > key {
                break;
            }
        }
        Ok(())
    }
    pub fn report(&self) -> Value {
        let (raw, index, total) = self.store.sizes();
        json!({"catalog_sha256":self.manifest["catalog_sha256"],"rows_sha256":self.rows_hash,"tables":self.relations.len(),"columns":self.columns,"rows":self.total_rows,"dbc_files":self.dbc_count,"dbc_bytes":self.dbc_bytes,"raw_bytes":raw,"index_bytes":index,"catalog_bytes":total,"complete":false,"read_calls":self.metrics.reads,"read_bytes":self.metrics.bytes,"max_read":self.metrics.max_read,"max_frame":self.metrics.max_frame,"decoded_row_bytes":self.metrics.decoded,"cache_peak":self.metrics.cache_peak,"index_depth":self.metrics.depth,"rss_kib":reader::rss_kib(),"open_seconds":self.seconds})
    }
}
fn hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
    out
}

/// Borrowed raw source fields. The caller explicitly copies to retain a field.
pub struct RowView<'a> {
    frame: &'a [u8],
    pub ordinal: u64,
    columns: usize,
    bitmap: usize,
}

/// One checked frame retained across recursive factory/catalog work.
pub struct RetainedCatalogRow {
    frame: Vec<u8>,
    ordinal: u64,
    columns: usize,
    bitmap: usize,
}
impl RetainedCatalogRow {
    pub fn view(&self) -> RowView<'_> {
        RowView {
            frame: &self.frame,
            ordinal: self.ordinal,
            columns: self.columns,
            bitmap: self.bitmap,
        }
    }
}
impl<'a> RowView<'a> {
    fn new(frame: &'a [u8], ordinal: u64, columns: usize) -> Result<Self> {
        check(frame.len() >= 16, "catalog.truncated")?;
        check(
            le(frame, 0, 4)? == frame.len() as u64 && frame.len() <= FRAME,
            "catalog.frame_limit",
        )?;
        check(le(frame, 4, 8)? == ordinal, "catalog.duplicate_row")?;
        let bitmap = le(frame, 14, 2)? as usize;
        check(
            le(frame, 12, 2)? == columns as u64 && bitmap == columns.div_ceil(8),
            "catalog.frame",
        )?;
        check(16 + bitmap <= frame.len(), "catalog.truncated")?;
        if !columns.is_multiple_of(8) {
            check(
                frame[16 + bitmap - 1] >> (columns % 8) == 0,
                "catalog.frame",
            )?;
        }
        Ok(Self {
            frame,
            ordinal,
            columns,
            bitmap,
        })
    }
    pub fn cells(&self) -> impl Iterator<Item = Result<Option<&'a [u8]>>> + '_ {
        let mut at = 16 + self.bitmap;
        (0..self.columns).map(move |i| {
            if self.frame[16 + i / 8] & (1 << (i % 8)) != 0 {
                return Ok(None);
            }
            let size = le(self.frame, at, 4)? as usize;
            at += 4;
            let end = at
                .checked_add(size)
                .ok_or_else(|| fail("catalog.overflow"))?;
            let bytes = self
                .frame
                .get(at..end)
                .ok_or_else(|| fail("catalog.truncated"))?;
            at = end;
            Ok(Some(bytes))
        })
    }
    fn validate(&self) -> Result<()> {
        let mut at = 16 + self.bitmap;
        for cell in self.cells() {
            if let Some(v) = cell? {
                at = at
                    .checked_add(4 + v.len())
                    .ok_or_else(|| fail("catalog.overflow"))?;
            }
        }
        check(at == self.frame.len(), "catalog.trailing")
    }
    fn key_matches(&self, columns: &[usize], key: &[u8]) -> Result<bool> {
        let mut cells = self.cells();
        let mut next = 0;
        let mut at = 0usize;
        for column in columns {
            if *column < next {
                cells = self.cells();
                next = 0;
            }
            let cell = cells
                .nth(*column - next)
                .ok_or_else(|| fail("catalog.index"))??;
            next = *column + 1;
            let length = cell.map_or(u32::MAX, |v| v.len() as u32);
            if key.get(at..at + 4) != Some(length.to_le_bytes().as_slice()) {
                return Ok(false);
            }
            at += 4;
            if let Some(bytes) = cell {
                if key.get(at..at + bytes.len()) != Some(bytes) {
                    return Ok(false);
                }
                at += bytes.len();
            }
            check(at <= FRAME, "catalog.long_key")?;
        }
        Ok(at == key.len())
    }
}

pub fn diagnose_catalog(path: &Path, candidate: bool) -> Result<Value> {
    Ok(VerifiedCatalog::open(path, candidate)?.report())
}
