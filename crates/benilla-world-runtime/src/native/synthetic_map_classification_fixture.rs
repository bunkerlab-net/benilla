// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete authored Map.dbc rows through the actual public loader; no install/admission.
use super::*;
use benilla_formats::{load_map_catalog, Chain, MapCatalog};
use std::sync::atomic::{AtomicU64, Ordering};
pub fn row(map_id: u32, map_type: u32) -> [u32; 42] {
    let mut row = [0; 42];
    row[0] = map_id;
    row[1] = 1;
    row[2] = map_type;
    row[4] = 1;
    row
}
pub fn catalog(map_id: u32, map_type: u32) -> MapCatalog {
    catalog_rows(&[row(map_id, map_type)])
}
pub fn catalog_rows(rows: &[[u32; 42]]) -> MapCatalog {
    let strings = b"\0AuthoredMap\0";
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, 42, 168, strings.len() as u32] {
        bytes.extend(word.to_le_bytes());
    }
    for row in rows {
        for word in row {
            bytes.extend(word.to_le_bytes());
        }
    }
    bytes.extend(strings);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-authored-map-kind-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(
        &path,
        synthetic_creature_model_fixture::archive(&[("DBFilesClient\\Map.dbc", bytes)]),
    )
    .unwrap();
    let mut chain = Chain::open(&path).unwrap();
    let catalog = load_map_catalog(&mut chain).unwrap();
    std::fs::remove_file(path).unwrap();
    catalog
}
