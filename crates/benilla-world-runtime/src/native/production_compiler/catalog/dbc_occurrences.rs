// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
const LOADERS: [&str; 2] = ["Entities/Transports:39", "Entities/Transports:99"];
fn projected(catalog: &mut VerifiedCatalog, schema: &Value) -> Result<[Vec<[u8; 32]>; 2]> {
    let cols = spawns::columns(schema, "transports")?;
    let entry = spawns::position(&cols, "entry")?;
    let name = spawns::position(&cols, "name")?;
    let period = spawns::position(&cols, "period")?;
    let mut hashes = [Vec::new(), Vec::new()];
    let mut names: BTreeMap<_, Vec<_>> = BTreeMap::new();
    let mut owned = 0;
    catalog.scan("transports", 0, 256, |row| {
        let mut cells = [None; 256];
        spawns::fill(&row, &mut cells)?;
        owned += cells[name].map_or(0, |c| c.len()) + 512;
        check(owned <= DECODED, "dbc.occurrence_limit")?;
        hashes[0].push(spawns::hash_cells(&[
            cells[entry],
            cells[name],
            cells[period],
        ]));
        if cells[entry].is_some() {
            names
                .entry(spawns::uint(cells[entry])?)
                .or_default()
                .push(cells[name].map(<[u8]>::to_vec));
        }
        Ok(())
    })?;
    let cols = spawns::columns(schema, "gameobject")?;
    let guid = spawns::position(&cols, "guid")?;
    let id = spawns::position(&cols, "id")?;
    catalog.scan("gameobject", 0, 256, |row| {
        let mut cells = [None; 256];
        spawns::fill(&row, &mut cells)?;
        let Some(id_raw) = cells[id] else {
            return Ok(());
        };
        if let Some(matches) = names.get(&spawns::uint(Some(id_raw))?) {
            for name in matches {
                check(
                    owned + (hashes[1].len() + 1) * 512 <= DECODED,
                    "dbc.occurrence_limit",
                )?;
                hashes[1].push(spawns::hash_cells(&[
                    cells[guid],
                    cells[id],
                    name.as_deref(),
                ]));
            }
        }
        Ok(())
    })?;
    for hashes in &mut hashes {
        hashes.sort_unstable();
    }
    Ok(hashes)
}
pub(super) fn verify(catalog: &mut VerifiedCatalog, schema: &Value, root: &Path) -> Result<Value> {
    let held = reader::Held::open(&root.join("occurrences.json"), CHUNK as u64)?;
    let mut raw = vec![0; held.len() as usize];
    held.read_into(0, &mut raw)?;
    let meta = bounded_json(&raw)?;
    catalog_schema::fields(
        &meta,
        &["format", "streams", "complete", "source_snapshot"],
        "dbc.occurrences",
    )?;
    check(
        meta["format"] == "benilla-transport-occurrences-v1"
            && meta["complete"] == false
            && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1",
        "dbc.occurrences",
    )?;
    catalog_schema::fields(&meta["streams"], &["0", "1"], "dbc.occurrences")?;
    let expected = projected(catalog, schema)?;
    let mut reports = serde_json::Map::new();
    for (i, loader) in LOADERS.iter().enumerate() {
        let name = i.to_string();
        let spec = &catalog_schema::POLICY["loaders"][*loader];
        let d = &meta["streams"][&name];
        reports.insert(
            name.clone(),
            verify_stream(&root.join(format!("{name}.rows")), &expected[i], spec, d)?,
        );
    }
    Ok(Value::Object(reports))
}
fn verify_stream(path: &Path, expected: &[[u8; 32]], spec: &Value, d: &Value) -> Result<Value> {
    catalog_schema::fields(
        d,
        &["query", "sha256", "rows", "bytes"],
        "dbc.occurrence_query",
    )?;
    check(d["query"] == spec["query"], "dbc.occurrence_query")?;
    let held = reader::Held::open(path, MAX_RAW)?;
    let mut buffer = vec![0; FRAME];
    let mut at = 0;
    let mut seen = Vec::new();
    let mut hash = Sha256::new();
    for ordinal in 0..expected.len() {
        check(at + 4 <= held.len(), "dbc.occurrence_exhausted")?;
        held.read_into(at, &mut buffer[..4])?;
        let size = le(&buffer, 0, 4)? as usize;
        check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
        held.read_into(at + 4, &mut buffer[4..size])?;
        let row = RowView::new(&buffer[..size], ordinal as u64, 3)?;
        row.validate()?;
        let mut cells = [None; 256];
        spawns::fill(&row, &mut cells)?;
        seen.push(spawns::hash_cells(&cells[..3]));
        hash.update(&buffer[..size]);
        at += size as u64;
    }
    check(at == held.len(), "dbc.occurrence_trailing")?;
    held.check()?;
    let hash = format!("{:x}", hash.finalize());
    check(
        d["rows"].as_u64() == Some(seen.len() as u64)
            && d["bytes"].as_u64() == Some(at)
            && d["sha256"] == hash,
        "dbc.occurrence_hash",
    )?;
    seen.sort_unstable();
    check(seen.as_slice() == expected, "dbc.occurrence_mismatch")?;
    let mut multiset = Sha256::new();
    for h in &seen {
        multiset.update(h);
    }
    Ok(
        json!({"source":spec["source"],"query":spec["query"],"rows":seen.len(),"occurrence_sha256":hash,"multiset_sha256":format!("{:x}",multiset.finalize())}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_transport_stream_refuses_forgery_exhaustion_trailing_and_ordinals() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "benilla-dbc-cursor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("rows");
        fn frame(ordinal: u64, cells: [&[u8]; 3]) -> Vec<u8> {
            let size = 17 + cells.iter().map(|c| 4 + c.len()).sum::<usize>();
            let mut out = Vec::new();
            out.extend_from_slice(&(size as u32).to_le_bytes());
            out.extend_from_slice(&ordinal.to_le_bytes());
            out.extend_from_slice(&3u16.to_le_bytes());
            out.extend_from_slice(&1u16.to_le_bytes());
            out.push(0);
            for cell in cells {
                out.extend_from_slice(&(cell.len() as u32).to_le_bytes());
                out.extend_from_slice(cell);
            }
            out
        }
        let a = [b"52".as_slice(), b"boat".as_slice(), b"99".as_slice()];
        let b = [b"53".as_slice(), b"ship".as_slice(), b"100".as_slice()];
        let mut expected = [
            spawns::hash_cells(&a.map(Some)),
            spawns::hash_cells(&b.map(Some)),
        ];
        expected.sort_unstable();
        let raw = [frame(0, b), frame(1, a)].concat();
        let spec = json!({"query":"SELECT entry, name, period FROM transports","source":"src/game/Entities/Transports.cpp:39"});
        let mut d = json!({"query":spec["query"],"rows":2,"bytes":raw.len(),"sha256":json::hash_bytes(&raw)});
        std::fs::write(&path, &raw).unwrap();
        assert_eq!(
            verify_stream(&path, &expected, &spec, &d).unwrap()["rows"],
            2
        );
        let mut cases = Vec::new();
        cases.push((raw[..raw.len() - 1].to_vec(), "catalog.truncated"));
        cases.push(([raw.as_slice(), b"x"].concat(), "dbc.occurrence_trailing"));
        let mut duplicate = raw.clone();
        let second = frame(0, b).len();
        duplicate[second + 4..second + 12].copy_from_slice(&0u64.to_le_bytes());
        cases.push((duplicate, "catalog.duplicate_row"));
        for (bytes, reason) in cases {
            std::fs::write(&path, &bytes).unwrap();
            assert_eq!(
                verify_stream(&path, &expected, &spec, &d).unwrap_err().id,
                reason
            );
        }
        let forged = [frame(0, [b"53", b"ship", b"101"]), frame(1, a)].concat();
        std::fs::write(&path, &forged).unwrap();
        d["sha256"] = json!(json::hash_bytes(&forged));
        assert_eq!(
            verify_stream(&path, &expected, &spec, &d).unwrap_err().id,
            "dbc.occurrence_mismatch"
        );
        d["query"] = json!("SELECT entry FROM transports");
        assert_eq!(
            verify_stream(&path, &expected, &spec, &d).unwrap_err().id,
            "dbc.occurrence_query"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
