// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    path: PathBuf,
    manifest: Value,
    relations: Value,
    policy: Value,
    chunk: Vec<u8>,
    ordinal: Vec<u8>,
    keypage: Vec<u8>,
}
fn header(magic: &[u8; 8], kind: u16, first: u64, count: u64) -> Vec<u8> {
    let mut out = magic.to_vec();
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&first.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out
}
fn row(ordinal: u64, key: &[u8], text: Option<&[u8]>) -> Vec<u8> {
    let size = 16 + 1 + 4 + key.len() + text.map_or(0, |v| 4 + v.len());
    let mut out = (size as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&ordinal.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.push(if text.is_some() { 0 } else { 2 });
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    if let Some(v) = text {
        out.extend_from_slice(&(v.len() as u32).to_le_bytes());
        out.extend_from_slice(v);
    }
    out
}
fn key_bytes(key: &[u8]) -> Vec<u8> {
    let mut out = (key.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(key);
    out
}
fn digest_bytes(h: &str) -> Vec<u8> {
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect()
}
impl Fixture {
    fn new(rows: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Self {
        let path = std::env::temp_dir().join(format!(
            "benilla-catalog-authored-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("objects")).unwrap();
        std::fs::create_dir(path.join("indexes")).unwrap();
        let table = json!({"columns":[{"COLUMN_NAME":"map"},{"COLUMN_NAME":"ScriptName"}],"indexes":[{"INDEX_NAME":"PRIMARY","SEQ_IN_INDEX":1,"COLUMN_NAME":"map"}]});
        let policy = json!({"core_revision":catalog_schema::POLICY["core_revision"],"database_revision":catalog_schema::POLICY["database_revision"],"tables":{"world_template":{"schema_sha256":json::digest(&table),"status":"consumed","loaders":["world"]}},"loaders":{"world":{}}});
        let schema = json!({"format":"benilla-classic-catalog-schema-v3","core_revision":policy["core_revision"],"database_revision":policy["database_revision"],"tables":{"world_template":table}});
        let coverage = json!({"policy_sha256":json::digest(&policy),"complete":false,"tables":{"world_template":{"status":"consumed","loaders":["world"]}}});
        let mut f = Self {
            path,
            manifest: Value::Null,
            relations: Value::Null,
            policy,
            chunk: Vec::new(),
            ordinal: Vec::new(),
            keypage: Vec::new(),
        };
        let mut chunks = Vec::new();
        for (chunk, batch) in rows.chunks(256).enumerate() {
            let start = (chunk * 256) as u64;
            let mut data = header(b"BNCAT3\0\0", 1, chunk as u64, batch.len() as u64);
            for (i, (key, text)) in batch.iter().enumerate() {
                data.extend(row(start + i as u64, key, text.as_deref()));
            }
            let payload = (data.len() - 32) as u32;
            data[28..32].copy_from_slice(&payload.to_le_bytes());
            let d = f.put(&data, false);
            if chunk == 0 {
                f.chunk = data;
            }
            chunks.push((
                d["sha256"].as_str().unwrap().to_owned(),
                start,
                batch.len() as u64,
                d["bytes"].as_u64().unwrap(),
            ));
        }
        let mut ord_level = Vec::new();
        for batch in chunks.chunks(84) {
            let mut p = header(b"BNCIDX3\0", 1, batch[0].1, batch.len() as u64);
            for (h, s, n, b) in batch {
                p.extend(digest_bytes(h));
                p.extend(s.to_le_bytes());
                p.extend((*n as u32).to_le_bytes());
                p.extend((*b as u32).to_le_bytes());
            }
            p.resize(4096, 0);
            let d = f.put(&p, true);
            if ord_level.is_empty() {
                f.ordinal = p;
            }
            ord_level.push((
                d["sha256"].as_str().unwrap().to_owned(),
                batch[0].1,
                batch.iter().map(|x| x.2).sum::<u64>(),
                4096,
            ));
        }
        if ord_level.is_empty() {
            let p = {
                let mut p = header(b"BNCIDX3\0", 1, 0, 0);
                p.resize(4096, 0);
                p
            };
            let d = f.put(&p, true);
            ord_level.push((d["sha256"].as_str().unwrap().to_owned(), 0, 0, 4096));
            f.ordinal = p;
        }
        let mut ord_height = 0;
        while ord_level.len() > 1 {
            let mut next = Vec::new();
            for batch in ord_level.chunks(84) {
                let mut p = header(b"BNCIDX3\0", 2, batch[0].1, batch.len() as u64);
                for (h, s, n, b) in batch {
                    p.extend(digest_bytes(h));
                    p.extend(s.to_le_bytes());
                    p.extend((*n as u32).to_le_bytes());
                    p.extend((*b as u32).to_le_bytes());
                }
                p.resize(4096, 0);
                let d = f.put(&p, true);
                next.push((
                    d["sha256"].as_str().unwrap().to_owned(),
                    batch[0].1,
                    batch.iter().map(|x| x.2).sum::<u64>(),
                    4096,
                ));
            }
            ord_level = next;
            ord_height += 1;
        }
        let ordinal =
            json!({"sha256":ord_level[0].0,"bytes":4096,"height":ord_height,"count":rows.len()});
        let mut keys: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(i, (k, _))| (key_bytes(k), i as u64))
            .collect();
        keys.sort();
        let mut key_level = Vec::new();
        for batch in keys.chunks(50) {
            let mut p = header(b"BNCIDX3\0", 3, 0, batch.len() as u64);
            for (k, o) in batch {
                p.extend(f.key_record(k, *o));
            }
            p.resize(4096, 0);
            let d = f.put(&p, true);
            if key_level.is_empty() {
                f.keypage = p;
            }
            key_level.push((
                batch.last().unwrap().clone(),
                d["sha256"].as_str().unwrap().to_owned(),
                batch.len() as u64,
            ));
        }
        if key_level.is_empty() {
            let mut p = header(b"BNCIDX3\0", 3, 0, 0);
            p.resize(4096, 0);
            let d = f.put(&p, true);
            key_level.push(((Vec::new(), 0), d["sha256"].as_str().unwrap().to_owned(), 0));
            f.keypage = p;
        }
        let mut key_height = 0;
        while key_level.len() > 1 {
            let mut next = Vec::new();
            for batch in key_level.chunks(33) {
                let mut p = header(b"BNCIDX3\0", 4, 0, batch.len() as u64);
                for ((k, o), h, n) in batch {
                    p.extend(f.key_record(k, *o));
                    p.extend(digest_bytes(h));
                    p.extend(n.to_le_bytes());
                }
                p.resize(4096, 0);
                let d = f.put(&p, true);
                next.push((
                    batch.last().unwrap().0.clone(),
                    d["sha256"].as_str().unwrap().to_owned(),
                    batch.iter().map(|x| x.2).sum::<u64>(),
                ));
            }
            key_level = next;
            key_height += 1;
        }
        let key =
            json!({"sha256":key_level[0].1,"bytes":4096,"height":key_height,"count":rows.len()});
        f.relations = json!({"world_template":{"id":1,"columns":2,"key_columns":[0],"rows":rows.len(),"ordinal":ordinal,"key":key}});
        let schema_root = f.put(&json::canonical(&schema), false);
        let loader_root = f.put(&json::canonical(&coverage), false);
        let dbc_root = f.put(b"{}", false);
        f.manifest = json!({"format":"benilla-classic-catalog-v3","core_revision":f.policy["core_revision"],"database_revision":f.policy["database_revision"],"source_snapshot":"phase0-sequential-diagnostic-not-F1","schema_root":schema_root,"loader_coverage_root":loader_root,"relation_root":null,"index_root":null,"dbc_root":dbc_root,"registration_root":null,"occurrence_root":null,"scope":{"primary_maps":[0,1],"global":true,"other_maps":true},"limits_policy":limits(),"completeness":false,"catalog_sha256":null});
        f.install();
        f
    }
    fn pilot() -> Self {
        Self::new(vec![
            (b"0".to_vec(), Some(b"Rust authored zero".to_vec())),
            (b"1".to_vec(), Some(b"Rust authored one".to_vec())),
        ])
    }
    fn put(&self, data: &[u8], index: bool) -> Value {
        let h = json::hash_bytes(data);
        std::fs::write(
            self.path
                .join(if index { "indexes" } else { "objects" })
                .join(&h),
            data,
        )
        .unwrap();
        json!({"sha256":h,"bytes":data.len()})
    }
    fn key_record(&self, k: &[u8], ordinal: u64) -> Vec<u8> {
        let mut out = (k.len() as u32).to_le_bytes().to_vec();
        out.extend(0u32.to_le_bytes());
        out.extend(ordinal.to_le_bytes());
        if k.len() <= 64 {
            out.extend(k);
        } else {
            let d = self.put(k, false);
            out.extend(digest_bytes(d["sha256"].as_str().unwrap()));
        }
        out.resize(80, 0);
        out
    }
    fn install(&mut self) {
        self.manifest["relation_root"] = self.put(&json::canonical(&self.relations), false);
        self.manifest["index_root"] = json!(json::digest(
            &json!({"world_template":{"ordinal":self.relations["world_template"]["ordinal"],"key":self.relations["world_template"]["key"]}})
        ));
        self.save();
    }
    fn save(&mut self) {
        let mut m = self.manifest.clone();
        m.as_object_mut().unwrap().remove("catalog_sha256");
        self.manifest["catalog_sha256"] = json!(json::digest(&m));
        std::fs::write(
            self.path.join("candidate.json"),
            json::canonical(&self.manifest),
        )
        .unwrap();
    }
    fn replace_page(&mut self, member: &str, data: &[u8]) {
        let d = self.put(data, true);
        self.relations["world_template"][member]["sha256"] = d["sha256"].clone();
        self.install();
    }
    fn replace_chunk(&mut self, data: &[u8]) {
        let d = self.put(data, false);
        let mut p = self.ordinal.clone();
        p[32..64].copy_from_slice(&digest_bytes(d["sha256"].as_str().unwrap()));
        p[76..80].copy_from_slice(&(data.len() as u32).to_le_bytes());
        self.replace_page("ordinal", &p);
    }
    fn open(&self) -> Result<VerifiedCatalog> {
        VerifiedCatalog::open_policy(&self.path, true, CACHE, &self.policy)
    }
    fn refuses(&self, id: &str) {
        match self.open() {
            Ok(_) => panic!("accepted {id}"),
            Err(e) => assert_eq!(e.id, id),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
#[test]
fn catalog_authored_scan_lookup_and_resource_contract() {
    let f = Fixture::pilot();
    let mut c = f.open().unwrap();
    let mut seen = Vec::new();
    c.scan("world_template", 0, 256, |r| {
        seen.push((
            r.ordinal,
            r.cells()
                .map(|v| v.unwrap().unwrap().to_vec())
                .collect::<Vec<_>>(),
        ));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        seen[0],
        (0, vec![b"0".to_vec(), b"Rust authored zero".to_vec()])
    );
    assert_eq!(seen.len(), 2);
    let mut hits = Vec::new();
    c.lookup("world_template", &key_bytes(b"1"), |n| {
        hits.push(n);
        Ok(())
    })
    .unwrap();
    assert_eq!(hits, vec![1]);
    c.lookup("world_template", &key_bytes(b"9"), |_| {
        panic!("false presence")
    })
    .unwrap();
    assert_eq!(
        c.scan("world_template", 0, 257, |_| Ok(())).unwrap_err().id,
        "catalog.batch_limit"
    );
    assert_eq!(
        c.scan("", 0, 1, |_| Ok(())).unwrap_err().id,
        "catalog.table"
    );
    assert_eq!(
        c.lookup("world_template", &vec![0; 65537], |_| Ok(()))
            .unwrap_err()
            .id,
        "catalog.long_key"
    );
    assert_eq!(c.report()["rows"], 2);
    assert_eq!(c.report()["complete"], false);
    assert!(c.metrics.max_read <= CHUNK);
    assert!(c.metrics.cache_peak <= CACHE);
    match VerifiedCatalog::open_policy(&f.path, true, 32, &f.policy) {
        Err(e) => assert_eq!(e.id, "catalog.cache_limit"),
        Ok(_) => panic!("cache ceiling ignored"),
    }
}
#[test]
fn catalog_authored_truncation_overflow_hash_and_version_refuse() {
    let mut f = Fixture::pilot();
    let truncated = f.chunk[..f.chunk.len() - 1].to_vec();
    f.replace_chunk(&truncated);
    f.refuses("catalog.truncated");
    let mut f = Fixture::pilot();
    f.manifest["schema_root"]["bytes"] = serde_json::from_str("18446744073709551616").unwrap();
    f.save();
    f.refuses("catalog.overflow");
    let f = Fixture::pilot();
    let mut chunk = f.chunk.clone();
    *chunk.last_mut().unwrap() ^= 1;
    let h = hex(&f.ordinal[32..64]);
    std::fs::write(f.path.join("objects").join(h), chunk).unwrap();
    f.refuses("catalog.hash");
    let mut f = Fixture::pilot();
    f.manifest["format"] = json!("benilla-classic-source-v2");
    f.save();
    f.refuses("catalog.version");
}
#[test]
fn catalog_authored_forged_indexes_holes_overlap_duplicates_and_long_keys_refuse() {
    let mut f = Fixture::pilot();
    let mut p = f.keypage.clone();
    p[52] = b'2';
    f.replace_page("key", &p);
    f.refuses("catalog.index");
    for start in [1u64, 2] {
        let mut f = Fixture::pilot();
        let mut p = f.ordinal.clone();
        p[64..72].copy_from_slice(&start.to_le_bytes());
        f.replace_page("ordinal", &p);
        f.refuses("catalog.range");
    }
    let mut f = Fixture::pilot();
    f.relations["world_template"]["key"] = f.relations["world_template"]["ordinal"].clone();
    f.install();
    f.refuses("catalog.duplicate_descriptor");
    let mut f = Fixture::pilot();
    let mut chunk = f.chunk.clone();
    let first = u32::from_le_bytes(chunk[32..36].try_into().unwrap()) as usize;
    chunk[32 + first + 4..32 + first + 12].copy_from_slice(&0u64.to_le_bytes());
    f.replace_chunk(&chunk);
    f.refuses("catalog.duplicate_row");
    let mut f = Fixture::pilot();
    let mut p = f.keypage.clone();
    p[32..36].copy_from_slice(&65537u32.to_le_bytes());
    f.replace_page("key", &p);
    f.refuses("catalog.long_key");
}
#[cfg(unix)]
#[test]
fn catalog_authored_paths_symlinks_and_mutation_refuse() {
    let mut f = Fixture::pilot();
    f.manifest["schema_root"]["sha256"] = json!("../escape");
    f.save();
    f.refuses("catalog.path");
    let f = Fixture::pilot();
    let h = hex(&f.ordinal[32..64]);
    let object = f.path.join("objects").join(&h);
    let original = f.path.join("original");
    std::fs::rename(&object, &original).unwrap();
    std::os::unix::fs::symlink(&original, &object).unwrap();
    f.refuses("catalog.path");
    let f = Fixture::pilot();
    let alias = f.path.join("alias");
    std::os::unix::fs::symlink(&f.path, &alias).unwrap();
    VerifiedCatalog::open_policy(&alias, true, CACHE, &f.policy).unwrap();
    let f = Fixture::pilot();
    let mut c = f.open().unwrap();
    let h = hex(&f.ordinal[32..64]);
    let mut chunk = f.chunk.clone();
    *chunk.last_mut().unwrap() ^= 1;
    std::fs::write(f.path.join("objects").join(h), chunk).unwrap();
    assert_eq!(
        c.scan("world_template", 0, 1, |_| Ok(())).unwrap_err().id,
        "catalog.changed"
    );
}
#[test]
fn catalog_authored_long_keys_duplicate_values_and_tree_boundaries_survive() {
    let f = Fixture::new(vec![(vec![b'x'; 1000], None); 120]);
    let mut c = f.open().unwrap();
    let mut hits = Vec::new();
    c.lookup("world_template", &key_bytes(&vec![b'x'; 1000]), |n| {
        hits.push(n);
        Ok(())
    })
    .unwrap();
    assert_eq!(hits, (0..120).collect::<Vec<_>>());
    let f = Fixture::new(
        (0u32..22000)
            .map(|n| (n.to_le_bytes().to_vec(), None))
            .collect(),
    );
    let mut c = f.open().unwrap();
    let mut count = 0;
    c.scan("world_template", 21990, 256, |_| {
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 10);
    assert!(c.metrics.depth >= 3);
    c.lookup("world_template", &key_bytes(&21999u32.to_le_bytes()), |n| {
        assert_eq!(n, 21999);
        Ok(())
    })
    .unwrap();
}

#[test]
fn catalog_authored_metadata_budgets_precede_allocation() {
    let f = Fixture::pilot();
    let deep = format!("{}0{}", "[".repeat(34), "]".repeat(34));
    let nodes = format!("[{}0]", "0,".repeat(100_000));
    for raw in [deep, nodes] {
        std::fs::write(f.path.join("candidate.json"), raw).unwrap();
        f.refuses("catalog.json_limit");
    }
    std::fs::write(f.path.join("candidate.json"), br#"{"format":0,"format":1}"#).unwrap();
    f.refuses("json.duplicate_field");
}

#[test]
fn catalog_authored_unsigned_descriptors_reject_booleans() {
    let mut f = Fixture::pilot();
    f.manifest["scope"]["primary_maps"][0] = json!(false);
    f.save();
    f.refuses("catalog.scope");
    for (field, value, id) in [
        ("id", json!(true), "catalog.overflow"),
        (
            "rows",
            serde_json::from_str("18446744073709551616").unwrap(),
            "catalog.overflow",
        ),
        ("key_columns", json!([false]), "catalog.index"),
    ] {
        let mut f = Fixture::pilot();
        f.relations["world_template"][field] = value;
        f.install();
        f.refuses(id);
    }
}
