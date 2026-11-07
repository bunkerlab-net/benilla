//! Authored structural Map.dbc records through the actual patch-chain loader.
use super::*;
use crate::authored_mpq_crypto as crypto;
use std::sync::atomic::{AtomicU64, Ordering};

fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    const SLOTS: u32 = 8;
    assert!(members.len() < SLOTS as usize);
    let hash_pos = 32;
    let block_pos = hash_pos + SLOTS * 16;
    let mut data_pos = block_pos + members.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; SLOTS as usize * 4];
    let mut blocks = Vec::new();
    for (index, (name, data)) in members.iter().enumerate() {
        let mut slot = (hash_string(name, hash_type::TABLE_OFFSET) & (SLOTS - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (SLOTS as usize - 1);
        }
        hashes[slot * 4..slot * 4 + 4].copy_from_slice(&[
            hash_string(name, hash_type::NAME_A),
            hash_string(name, hash_type::NAME_B),
            0,
            index as u32,
        ]);
        blocks.extend([data_pos, data.len() as u32, data.len() as u32, 0x8000_0000]);
        data_pos += data.len() as u32;
    }
    encrypt_block(
        &mut hashes,
        hash_string("(hash table)", hash_type::FILE_KEY),
    );
    encrypt_block(
        &mut blocks,
        hash_string("(block table)", hash_type::FILE_KEY),
    );
    let mut bytes = vec![0; 32];
    for (offset, value) in [
        (0, 0x1a51_504d_u32),
        (4, 32),
        (8, data_pos),
        (16, hash_pos),
        (20, block_pos),
        (24, SLOTS),
        (28, members.len() as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for (_, data) in members {
        bytes.extend_from_slice(data);
    }
    bytes
}

fn dbc(rows: &[[u32; 42]]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, 42, 168, 1] {
        bytes.extend(word.to_le_bytes());
    }
    for row in rows {
        for word in row {
            bytes.extend(word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}
fn load(bytes: Option<Vec<u8>>) -> Result<MapCatalog> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-map-raw-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let members = match bytes {
        Some(bytes) => vec![(MAP, bytes)],
        None => vec![("unrelated", vec![0])],
    };
    std::fs::write(&path, archive(&members)).unwrap();
    let result = Chain::open(&path).and_then(|mut chain| load_map_catalog(&mut chain));
    std::fs::remove_file(path).unwrap();
    result
}
#[test]
fn complete_map_records_preserve_full_width_fields_and_duplicate_order() {
    let mut first = [0; 42];
    for (column, value) in first.iter_mut().enumerate() {
        *value = 0x8000_0000 + column as u32;
    }
    // Keep the legacy parser's string offsets valid; raw values in all other
    // columns, including uninterpreted type and float bits, remain untouched.
    for column in [1, 4, 20, 29] {
        first[column] = 0;
    }
    first[0] = u32::MAX;
    first[2] = 0xffff_fffe;
    let mut duplicate = first;
    duplicate[2] = 0x1234_5678;
    let rows = [first, duplicate, first];
    let catalog = load(Some(dbc(&rows))).unwrap();
    assert_eq!(catalog.raw_rows(), Some(rows.as_slice()));
    assert_eq!(catalog.instance_type(u32::MAX), Some(first[2]));
    assert_eq!(catalog.directory(u32::MAX), None);
}
#[test]
fn complete_empty_map_table_is_known_empty_not_missing() {
    let catalog = load(Some(dbc(&[]))).unwrap();
    assert_eq!(catalog.raw_rows(), Some([].as_slice()));
    assert!(catalog.is_empty());
    assert!(load(None).is_err());
}
#[test]
fn malformed_map_coverage_never_exposes_partial_raw_rows() {
    let bytes = dbc(&[[0; 42]]);
    for mutation in 0..9 {
        let mut malformed = bytes.clone();
        match mutation {
            0 => {
                malformed.pop();
            }
            1 => malformed.truncate(25),
            2 => malformed.push(0),
            3 => malformed[8..12].copy_from_slice(&41u32.to_le_bytes()),
            4 => malformed[12..16].copy_from_slice(&164u32.to_le_bytes()),
            5 => malformed[12..16].copy_from_slice(&172u32.to_le_bytes()),
            6 => malformed[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
            7 => malformed[16..20].copy_from_slice(&u32::MAX.to_le_bytes()),
            8 => malformed[..4].copy_from_slice(b"BAD!"),
            _ => unreachable!(),
        }
        // The existing general loader may reject malformed input itself. If it
        // accepts it for legacy lookup, the complete raw view must remain absent.
        if let Ok(catalog) = load(Some(malformed)) {
            assert!(catalog.raw_rows().is_none(), "mutation {mutation}");
        }
    }
}
