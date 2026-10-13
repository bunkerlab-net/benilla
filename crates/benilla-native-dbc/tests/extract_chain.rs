// SPDX-License-Identifier: GPL-3.0-or-later
//! Newly authored synthetic MPQ/DBC bytes only; no game rows or assets.
use benilla_native_dbc::{extract, MAX_FILE_BYTES, TABLES};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// Reuse the repository's test-only table encoder, not a second MPQ algorithm.
#[path = "../../benilla-mpq/src/crypto.rs"]
mod crypto;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "native-dbc-chain-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn data(&self) -> PathBuf {
        let data = self.0.join("Data");
        fs::create_dir(&data).unwrap();
        data
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Entry {
    name: String,
    bytes: Vec<u8>,
    flags: u32,
    declared: u32,
}
fn dbc(fields: u32, size: u32, marker: u8) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [1u32, fields, size, 1] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.resize(20 + size as usize + 1, 0);
    bytes[20] = marker;
    bytes
}
fn entry(table: &str, marker: u8) -> Entry {
    let (_, fields, size) = TABLES.iter().find(|(name, _, _)| *name == table).unwrap();
    let bytes = dbc(*fields, *size, marker);
    Entry {
        name: format!("DBFilesClient\\{table}.dbc"),
        declared: bytes.len() as u32,
        bytes,
        flags: 0x80000000,
    }
}
fn archive(entries: &[Entry]) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    let slots = 64u32;
    let hash_pos = 32u32;
    let block_pos = hash_pos + slots * 16;
    let mut data_pos = block_pos + entries.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; slots as usize * 4];
    let mut blocks = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let mut slot = (hash_string(&entry.name, hash_type::TABLE_OFFSET) & (slots - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (slots as usize - 1);
        }
        hashes[slot * 4..slot * 4 + 4].copy_from_slice(&[
            hash_string(&entry.name, hash_type::NAME_A),
            hash_string(&entry.name, hash_type::NAME_B),
            0,
            index as u32,
        ]);
        blocks.extend([
            data_pos,
            entry.bytes.len() as u32,
            entry.declared,
            entry.flags,
        ]);
        data_pos += entry.bytes.len() as u32;
    }
    encrypt_block(
        &mut hashes,
        hash_string("(hash table)", hash_type::FILE_KEY),
    );
    encrypt_block(
        &mut blocks,
        hash_string("(block table)", hash_type::FILE_KEY),
    );
    let mut bytes = vec![0u8; 32];
    for (offset, value) in [
        (0, 0x1a51504d_u32),
        (16, hash_pos),
        (20, block_pos),
        (24, slots),
        (28, entries.len() as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend(word.to_le_bytes());
    }
    for entry in entries {
        bytes.extend(&entry.bytes);
    }
    bytes
}
fn base() -> Vec<u8> {
    archive(
        &TABLES
            .iter()
            .map(|(table, _, _)| entry(table, 1))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn actual_cli_uses_patch_winner_preserves_source_and_exports_fixed_allowlist() {
    let temp = Temp::new();
    let data = temp.data();
    let output = temp.0.join("private");
    let original = base();
    fs::write(data.join("dbc.MPQ"), &original).unwrap();
    fs::write(data.join("patch.MPQ"), archive(&[entry("Map", 2)])).unwrap();
    fs::write(data.join("patch-2.MPQ"), archive(&[entry("Map", 3)])).unwrap();
    fs::write(data.join("patch-10.MPQ"), archive(&[entry("Map", 9)])).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_benilla-native-dbc"))
        .args([
            "--data",
            data.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(output.join("Map.dbc")).unwrap()[20], 3);
    assert_eq!(fs::read(data.join("dbc.MPQ")).unwrap(), original);
    assert_eq!(fs::read_dir(&output).unwrap().count(), TABLES.len() + 1);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("extraction.json")).unwrap()).unwrap();
    let map = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == "Map.dbc")
        .unwrap();
    assert_eq!(map["winning_archive"], "patch-2.MPQ");
    assert_eq!(manifest["client_build_authenticity_verified"], false);
    assert!(extract(&data, &output).is_err());
}

#[test]
fn tombstone_does_not_fall_back_to_base_or_publish_partial_output() {
    let temp = Temp::new();
    let data = temp.data();
    let output = temp.0.join("private");
    fs::write(data.join("dbc.MPQ"), base()).unwrap();
    let mut deleted = entry("Spell", 0);
    deleted.flags = 0x82000000;
    deleted.bytes.clear();
    deleted.declared = 0;
    fs::write(data.join("patch.MPQ"), archive(&[deleted])).unwrap();
    assert!(extract(&data, &output).is_err());
    assert!(!output.exists());
}

#[test]
fn size_is_capped_before_member_read_or_decompression() {
    let temp = Temp::new();
    let data = temp.data();
    let output = temp.0.join("private");
    fs::write(data.join("dbc.MPQ"), base()).unwrap();
    let mut oversized = entry("CharBaseInfo", 1);
    oversized.declared = MAX_FILE_BYTES as u32 + 1;
    fs::write(data.join("patch.MPQ"), archive(&[oversized])).unwrap();
    let error = extract(&data, &output).unwrap_err();
    assert!(
        error.to_string().contains("input.member_byte_limit"),
        "{error:#}"
    );
    assert!(!output.exists());
}

#[test]
fn invalid_wdbc_winner_is_not_replaced_with_base() {
    let temp = Temp::new();
    let data = temp.data();
    let output = temp.0.join("private");
    fs::write(data.join("dbc.MPQ"), base()).unwrap();
    let mut corrupt = entry("Map", 2);
    corrupt.bytes[8] = 41;
    fs::write(data.join("patch.MPQ"), archive(&[corrupt])).unwrap();
    assert!(extract(&data, &output).is_err());
    assert!(!output.exists());
}

#[test]
fn bounded_chain_enforces_cumulative_indices_and_mount_counts() {
    use benilla_formats::{Chain, ChainLimits};
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), base()).unwrap();
    fs::write(data.join("patch.MPQ"), archive(&[entry("Map", 3)])).unwrap();
    let limits = ChainLimits {
        index_bytes: 2000,
        sector_bytes: 4096,
        member_bytes: MAX_FILE_BYTES,
        header_scan_bytes: 1024,
        archives: 2,
        directory_entries: 2,
    };
    let error = Chain::open_bounded(&data, limits)
        .err()
        .expect("combined index budget must reject");
    assert!(format!("{error:#}").contains("bounded index byte limit"));
    assert!(Chain::open_bounded(
        &data,
        ChainLimits {
            index_bytes: 4096,
            ..limits
        }
    )
    .is_ok());
    assert!(Chain::open_bounded(
        &data,
        ChainLimits {
            index_bytes: 4096,
            archives: 1,
            ..limits
        }
    )
    .is_err());
    assert!(Chain::open_bounded(
        &data,
        ChainLimits {
            index_bytes: 4096,
            directory_entries: 1,
            ..limits
        }
    )
    .is_err());
}
