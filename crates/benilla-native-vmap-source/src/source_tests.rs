// SPDX-License-Identifier: GPL-3.0-or-later
//! Newly authored MPQ fixture bytes only; no client install, assets, or rows.

use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

// The same test-only table encoder used by benilla-native-dbc/extract_chain.rs.
#[path = "../../benilla-mpq/src/crypto.rs"]
mod crypto;

const MAP: &str = "DBFilesClient\\Map.dbc";

pub(crate) struct Temp(pub(crate) PathBuf);
impl Temp {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "native-vmap-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub(crate) fn data(&self) -> PathBuf {
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

pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) flags: u32,
    pub(crate) declared: u32,
}

pub(crate) fn entry(name: &str, marker: u8) -> Entry {
    raw_entry(name, vec![marker, 0, 2, 3, 4, 5, 6, 7])
}

pub(crate) fn raw_entry(name: &str, bytes: Vec<u8>) -> Entry {
    Entry {
        name: name.to_owned(),
        declared: bytes.len() as u32,
        bytes,
        flags: 0x8000_0000,
    }
}

pub(crate) fn archive(entries: &[Entry]) -> Vec<u8> {
    archive_version(entries, 0)
}

pub(crate) fn archive_version(entries: &[Entry], version: u16) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    let slots = 64u32;
    let header_bytes = match version {
        0 => 32u32,
        1 => 44,
        _ => panic!("fixture version"),
    };
    let hash_pos = header_bytes;
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
    let mut bytes = vec![0u8; header_bytes as usize];
    for (offset, value) in [
        (0, 0x1a51_504d_u32),
        (4, header_bytes),
        (8, data_pos),
        (16, hash_pos),
        (20, block_pos),
        (24, slots),
        (28, entries.len() as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[12..14].copy_from_slice(&version.to_le_bytes());
    bytes[14..16].copy_from_slice(&3u16.to_le_bytes());
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend(word.to_le_bytes());
    }
    for entry in entries {
        bytes.extend(&entry.bytes);
    }
    bytes
}

fn store(data: &Path, name: &str, entries: &[Entry]) {
    fs::write(data.join(name), archive(entries)).unwrap();
}

fn present(lookup: Lookup) -> CapturedMember {
    match lookup {
        Lookup::Present(member) => member,
        _ => panic!("positive member required"),
    }
}

#[test]
fn numeric_patch_order_wins_instead_of_lexical_or_fixed_five() {
    let temp = Temp::new();
    let data = temp.data();
    store(&data, "patch-2.MPQ", &[entry(MAP, 2)]);
    store(&data, "patch-10.MPQ", &[entry(MAP, 10)]);
    store(&data, "patch.MPQ", &[entry(MAP, 1)]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(
        source.archives(),
        &["patch.MPQ", "patch-2.MPQ", "patch-10.MPQ"]
    );
    let value = present(source.read(MAP, &|| false).unwrap());
    assert_eq!(value.bytes()[0], 10);
    assert_eq!(value.member().winning_archive, "patch-10.MPQ");
    assert_eq!(source.receipts()[0].probe_count, 1);
    source.verify_unchanged(&|| false).unwrap();
}

#[test]
fn base_mount_order_is_exact_and_missing_archives_are_skipped() {
    let temp = Temp::new();
    let data = temp.data();
    for (index, name) in BASE_ARCHIVES.iter().enumerate() {
        store(&data, name, &[entry(MAP, index as u8)]);
    }
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(source.archives(), BASE_ARCHIVES);
    assert_eq!(
        present(source.read(MAP, &|| false).unwrap())
            .member()
            .winning_archive,
        "base.MPQ"
    );
    fs::remove_file(data.join("base.MPQ")).unwrap();
    assert_eq!(
        source.verify_unchanged(&|| false),
        Err(SourceError::Changed)
    );
    let mut fresh = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(
        present(fresh.read(MAP, &|| false).unwrap())
            .member()
            .winning_archive,
        "wmo.MPQ"
    );
}

#[test]
fn patch_99_included_but_100_dbc_misc_and_noncanonical_patch_ignored() {
    let temp = Temp::new();
    let data = temp.data();
    store(&data, "patch-99.MPQ", &[entry(MAP, 99)]);
    for ignored in ["patch-100.MPQ", "patch-01.MPQ", "dbc.MPQ", "misc.MPQ"] {
        fs::write(data.join(ignored), b"not even an archive").unwrap();
    }
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(source.archives(), &["patch-99.MPQ"]);
    assert_eq!(present(source.read(MAP, &|| false).unwrap()).bytes()[0], 99);
    source.verify_unchanged(&|| false).unwrap();
}

#[test]
fn zero_one_and_delete_dummies_block_lower_winner_without_payload_read() {
    for (bytes, flags) in [
        (vec![], 0x8000_0000),
        (vec![7], 0x8000_0000),
        (vec![], 0x8200_0000),
    ] {
        let temp = Temp::new();
        let data = temp.data();
        store(&data, "model.MPQ", &[entry(MAP, 8)]);
        let mut dummy = raw_entry(MAP, bytes.clone());
        dummy.flags = flags;
        store(&data, "patch-10.MPQ", &[dummy]);
        let mut source = CapturedSource::open(&data, &|| false).unwrap();
        assert!(matches!(
            source.read(MAP, &|| false).unwrap(),
            Lookup::Dummy {}
        ));
        match &source.receipts().last().unwrap().outcome {
            Outcome::Dummy {
                winning_archive,
                bytes: length,
            } => {
                assert_eq!(winning_archive, "patch-10.MPQ");
                assert_eq!(*length, bytes.len());
            }
            _ => panic!("dummy must block fallback"),
        }
        assert_eq!(source.read_work_bytes(), 0);
        source.verify_unchanged(&|| false).unwrap();
    }
}

#[test]
fn absence_is_distinct_and_receipts_preserve_order_and_repeats() {
    let temp = Temp::new();
    let data = temp.data();
    store(&data, "model.MPQ", &[entry(MAP, 1)]);
    store(&data, "patch-3.MPQ", &[]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert!(matches!(
        source.read("missing.wmo", &|| false).unwrap(),
        Lookup::Absent
    ));
    let original = present(source.read(MAP, &|| false).unwrap());
    assert_eq!(source.receipts()[1].probe_count, 2);
    source.read("missing.wmo", &|| false).unwrap();
    assert_eq!(
        source
            .receipts()
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["missing.wmo", MAP, "missing.wmo"]
    );
    assert_eq!(source.read_count(), 3);
    assert_eq!(source.read_work_bytes(), original.bytes().len() as u64 * 2);
    assert_eq!(source.binding_digest().len(), 64);
    source.verify_unchanged(&|| false).unwrap();
}

#[test]
fn positive_bytes_are_immutable_and_recheck_detects_same_length_mutation() {
    let temp = Temp::new();
    let data = temp.data();
    store(&data, "model.MPQ", &[entry(MAP, 1)]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    let original = present(source.read(MAP, &|| false).unwrap());
    store(&data, "model.MPQ", &[entry(MAP, 2)]);
    assert_eq!(original.bytes()[0], 1);
    assert_eq!(
        source.verify_unchanged(&|| false),
        Err(SourceError::Changed)
    );
}

#[test]
fn recheck_detects_absence_dummy_winner_and_mount_changes() {
    for change in 0..5 {
        let temp = Temp::new();
        let data = temp.data();
        store(&data, "model.MPQ", &[entry(MAP, 1)]);
        if change == 1 {
            store(&data, "patch.MPQ", &[raw_entry(MAP, vec![])]);
        }
        let mut source = CapturedSource::open(&data, &|| false).unwrap();
        source
            .read(if change == 0 { "absent" } else { MAP }, &|| false)
            .unwrap();
        match change {
            0 => store(&data, "model.MPQ", &[entry("absent", 1)]),
            1 => store(&data, "patch.MPQ", &[entry(MAP, 1)]),
            2 => store(&data, "model.MPQ", &[raw_entry(MAP, vec![1])]),
            3 => store(&data, "patch-10.MPQ", &[entry(MAP, 1)]),
            4 => store(&data, "texture.MPQ", &[]),
            _ => unreachable!(),
        }
        assert_eq!(
            source.verify_unchanged(&|| false),
            Err(SourceError::Changed)
        );
    }
}

#[test]
fn same_receipts_in_different_order_have_different_bindings() {
    let temp = Temp::new();
    let data = temp.data();
    let mut left = CapturedSource::open(&data, &|| false).unwrap();
    let mut right = CapturedSource::open(&data, &|| false).unwrap();
    for name in ["a", "b"] {
        left.read(name, &|| false).unwrap();
    }
    for name in ["b", "a"] {
        right.read(name, &|| false).unwrap();
    }
    assert_ne!(left.binding_digest(), right.binding_digest());
}

#[test]
fn cancellation_precedes_filesystem_reads_and_member_work() {
    let temp = Temp::new();
    let data = temp.data();
    assert_eq!(
        CapturedSource::open(&data.join("missing"), &|| true).unwrap_err(),
        SourceError::Cancelled
    );
    store(&data, "base.MPQ", &[entry(MAP, 1)]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(
        source.read(MAP, &|| true).unwrap_err(),
        SourceError::Cancelled
    );
    assert_eq!(source.read_count(), 0);
    assert_eq!(source.read_work_bytes(), 0);
    assert_eq!(
        source.verify_unchanged(&|| true),
        Err(SourceError::Cancelled)
    );
}

#[test]
fn caller_count_total_work_member_name_and_member_bytes_are_bounded() {
    let temp = Temp::new();
    let data = temp.data();
    store(&data, "model.MPQ", &[entry(MAP, 1)]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    for name in ["", "../escape", "a\\..\\b", "a//b", "x\n", "/absolute"] {
        assert_eq!(
            source.read(name, &|| false).unwrap_err(),
            SourceError::InvalidName
        );
    }
    source.read_work = MAX_READ_WORK_BYTES - 1;
    assert_eq!(
        source.read(MAP, &|| false).unwrap_err(),
        SourceError::ReadWorkLimit
    );
    source.read_work = 0;
    for _ in 0..MAX_LOOKUPS {
        source.read("missing", &|| false).unwrap();
    }
    assert_eq!(
        source.read(MAP, &|| false).unwrap_err(),
        SourceError::LookupLimit
    );
    let mut huge = entry(MAP, 1);
    huge.declared = MAX_MEMBER_BYTES as u32 + 1;
    store(&data, "model.MPQ", &[huge]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(
        source.read(MAP, &|| false).unwrap_err(),
        SourceError::MemberLimit
    );
}

#[test]
fn bounded_header_versions_zero_and_one_normalize_declared_size() {
    for version in [0, 1] {
        let temp = Temp::new();
        let data = temp.data();
        let mut bytes = archive_version(&[entry(MAP, 1)], version);
        bytes[4..8].copy_from_slice(&0xdead_beefu32.to_le_bytes());
        fs::write(data.join("base.MPQ"), bytes).unwrap();
        let mut source = CapturedSource::open(&data, &|| false).unwrap();
        assert_eq!(present(source.read(MAP, &|| false).unwrap()).bytes()[0], 1);
        source.verify_unchanged(&|| false).unwrap();
    }
}

#[test]
fn unsupported_header_shapes_fail_without_path_or_member_diagnostics() {
    for case in 0..5 {
        let temp = Temp::new();
        let data = temp.data();
        let mut bytes = archive_version(&[entry(MAP, 1)], 1);
        let expected = match case {
            0 => {
                bytes[12..14].copy_from_slice(&2u16.to_le_bytes());
                SourceError::HeaderVersion
            }
            1 => {
                bytes[32] = 1;
                SourceError::HeaderExtension
            }
            2 => {
                bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
                SourceError::HeaderSize
            }
            3 => {
                bytes[14..16].copy_from_slice(&8u16.to_le_bytes());
                SourceError::HeaderSector
            }
            4 => {
                bytes[24..28].copy_from_slice(&63u32.to_le_bytes());
                SourceError::HeaderIndex
            }
            _ => unreachable!(),
        };
        fs::write(data.join("base.MPQ"), bytes).unwrap();
        let error = CapturedSource::open(&data, &|| false).unwrap_err();
        assert_eq!(error, expected);
        assert!(!format!("{error:?} {error}").contains(data.to_str().unwrap()));
        assert!(!format!("{error:?} {error}").contains(MAP));
    }
}

#[test]
fn source_scan_ignores_userdata_pointer_and_uses_first_aligned_header() {
    let temp = Temp::new();
    let data = temp.data();
    let mut bytes = vec![0u8; 512];
    bytes[..4].copy_from_slice(b"MPQ\x1b");
    bytes[8..12].copy_from_slice(&32u32.to_le_bytes());
    bytes.extend(archive(&[entry(MAP, 3)]));
    fs::write(data.join("base.MPQ"), bytes).unwrap();
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(present(source.read(MAP, &|| false).unwrap()).bytes()[0], 3);
    assert_eq!(source.chain.archives[0].header_snapshot().0, 512);
}

#[test]
fn invalid_hash_and_unsupported_positive_flags_cannot_mint_absence_or_dummy() {
    for (flags, payload) in [
        (0, vec![]),
        (0x8001_0000, vec![1, 2]),
        (0x8200_0000, vec![1, 2]),
    ] {
        let temp = Temp::new();
        let data = temp.data();
        let mut bad = raw_entry(MAP, payload);
        bad.flags = flags;
        store(&data, "base.MPQ", &[bad]);
        let mut source = CapturedSource::open(&data, &|| false).unwrap();
        assert_eq!(
            source.read(MAP, &|| false).unwrap_err(),
            SourceError::MemberProfile
        );
        assert_eq!(source.read_count(), 0);
    }
    let temp = Temp::new();
    let data = temp.data();
    let mut bytes = archive(&[entry(MAP, 1)]);
    let key = crypto::hash_string("(hash table)", crypto::hash_type::FILE_KEY);
    let mut words: Vec<_> = bytes[32..1056]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word))
        .collect();
    crypto::decrypt_block(&mut words, key);
    let slot = (crypto::hash_string(MAP, crypto::hash_type::TABLE_OFFSET) & 63) as usize;
    words[slot * 4 + 3] = u32::MAX - 1;
    crypto::encrypt_block(&mut words, key);
    for (index, word) in words.iter().enumerate() {
        bytes[32 + index * 4..36 + index * 4].copy_from_slice(&word.to_le_bytes());
    }
    fs::write(data.join("base.MPQ"), bytes).unwrap();
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(
        source.read(MAP, &|| false).unwrap_err(),
        SourceError::MemberProfile
    );
}

#[test]
fn aliases_and_nonregular_archives_are_rejected() {
    let temp = Temp::new();
    let data = temp.data();
    store(&data, "BASE.mpq", &[]);
    assert_eq!(
        CapturedSource::open(&data, &|| false).unwrap_err(),
        SourceError::ArchiveCase
    );
    fs::remove_file(data.join("BASE.mpq")).unwrap();
    fs::create_dir(data.join("base.MPQ")).unwrap();
    assert_eq!(
        CapturedSource::open(&data, &|| false).unwrap_err(),
        SourceError::ArchiveNotRegular
    );
}

#[test]
fn listfile_reads_are_per_archive_with_scoped_ordered_receipts() {
    let temp = Temp::new();
    let data = temp.data();
    store(
        &data,
        "model.MPQ",
        &[raw_entry("(listfile)", b"base.wmo\r\n\0".to_vec())],
    );
    store(
        &data,
        "patch.MPQ",
        &[raw_entry("(listfile)", b"patch.wmo\r\n\0".to_vec())],
    );
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(
        present(
            source
                .read_from_archive(0, "(listfile)", &|| false)
                .unwrap()
        )
        .bytes(),
        b"base.wmo\r\n\0"
    );
    assert_eq!(
        present(
            source
                .read_from_archive(1, "(listfile)", &|| false)
                .unwrap()
        )
        .bytes(),
        b"patch.wmo\r\n\0"
    );
    assert_eq!(source.receipts()[0].probe_count, 1);
    source.verify_unchanged(&|| false).unwrap();
    store(
        &data,
        "model.MPQ",
        &[raw_entry("(listfile)", b"else.wmo\r\n\0".to_vec())],
    );
    assert_eq!(
        source.verify_unchanged(&|| false),
        Err(SourceError::Changed)
    );
}

#[test]
fn absent_listfile_never_falls_back_and_dummy_listfile_fails_closed() {
    let temp = Temp::new();
    let data = temp.data();
    store(
        &data,
        "model.MPQ",
        &[raw_entry("(listfile)", b"base.wmo\r\n\0".to_vec())],
    );
    store(&data, "patch.MPQ", &[]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert!(matches!(
        source
            .read_from_archive(1, "(listfile)", &|| false)
            .unwrap(),
        Lookup::Absent
    ));
    source.verify_unchanged(&|| false).unwrap();
    store(&data, "patch.MPQ", &[raw_entry("(listfile)", vec![])]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    assert_eq!(
        source
            .read_from_archive(1, "(listfile)", &|| false)
            .unwrap_err(),
        SourceError::DummyListfile
    );
    assert_eq!(source.read_count(), 0);
}

#[test]
fn borrowed_chain_receipts_preserve_exact_outcomes_order_and_epoch() {
    let temp = Temp::new();
    let data = temp.data();
    store(
        &data,
        "model.MPQ",
        &[entry(MAP, 1), raw_entry("dummy.wmo", vec![0])],
    );
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    source.read(MAP, &|| false).unwrap();
    source.read("absent.wmo", &|| false).unwrap();
    source.read("dummy.wmo", &|| false).unwrap();
    source.read(MAP, &|| false).unwrap();
    let epoch = source.binding_digest();
    let views: Vec<_> = source.source_receipts().collect();
    assert_eq!(
        views.iter().map(|r| r.name()).collect::<Vec<_>>(),
        [MAP, "absent.wmo", "dummy.wmo", MAP]
    );
    assert!(matches!(
        views[0].outcome(),
        crate::SourceOutcome::Present {
            winning_archive: "model.MPQ",
            bytes: 8,
            ..
        }
    ));
    assert!(matches!(views[1].outcome(), crate::SourceOutcome::Absent));
    assert!(matches!(
        views[2].outcome(),
        crate::SourceOutcome::Dummy {
            winning_archive: "model.MPQ",
            bytes: 1
        }
    ));
    assert!(views[0] == views[3]);
    assert_eq!(source.binding_digest(), epoch);
    source.verify_unchanged(&|| false).unwrap();
}

/// Reference replay retains the former owned-receipt comparison. Both paths
/// still obtain every observation from the same fresh chain and member reads.
fn verify_with_owned_receipts(source: &CapturedSource, cancel: &dyn Fn() -> bool) -> Result<()> {
    check_cancel(cancel)?;
    let chain = open_chain(&source.data, cancel)?;
    if chain.names != source.chain.names {
        return Err(SourceError::Changed);
    }
    let mut work = 0;
    for expected in &source.receipts {
        check_cancel(cancel)?;
        let (_, actual) = read_lookup(&chain, &expected.name, expected.scope, &mut work, cancel)?;
        if actual.into_receipt(&expected.name, expected.scope) != *expected {
            return Err(SourceError::Changed);
        }
    }
    Ok(())
}

#[test]
fn unnamed_observations_preserve_receipts_work_digest_and_cancellation_order() {
    use std::cell::Cell;
    let temp = Temp::new();
    let data = temp.data();
    store(
        &data,
        "model.MPQ",
        &[
            entry(MAP, 1),
            raw_entry("dummy.wmo", vec![0]),
            raw_entry("(listfile)", b"base.wmo\r\n\0".to_vec()),
        ],
    );
    store(
        &data,
        "patch.MPQ",
        &[raw_entry("(listfile)", b"patch.wmo\r\n\0".to_vec())],
    );
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    for name in ["missing.wmo", MAP, "dummy.wmo"] {
        source.read(name, &|| false).unwrap();
    }
    for index in 0..2 {
        source
            .read_from_archive(index, "(listfile)", &|| false)
            .unwrap();
    }
    for name in ["missing.wmo", MAP] {
        source.read(name, &|| false).unwrap();
    }
    let expected = [
        (LookupScope::Chain, "missing.wmo", 2),
        (LookupScope::Chain, MAP, 2),
        (LookupScope::Chain, "dummy.wmo", 2),
        (LookupScope::Archive(0), "(listfile)", 1),
        (LookupScope::Archive(1), "(listfile)", 1),
        (LookupScope::Chain, "missing.wmo", 2),
        (LookupScope::Chain, MAP, 2),
    ];
    for (receipt, (scope, name, probes)) in source.receipts.iter().zip(expected) {
        assert!(receipt.scope == scope);
        assert_eq!(receipt.name, name);
        assert_eq!(receipt.probe_count, probes);
    }
    assert!(matches!(source.receipts[0].outcome, Outcome::Absent));
    assert!(
        matches!(source.receipts[2].outcome, Outcome::Dummy { ref winning_archive, bytes: 1 } if winning_archive == "model.MPQ")
    );
    for (index, winner, payload) in [
        (1, "model.MPQ", &[1, 0, 2, 3, 4, 5, 6, 7][..]),
        (3, "model.MPQ", &b"base.wmo\r\n\0"[..]),
        (4, "patch.MPQ", &b"patch.wmo\r\n\0"[..]),
        (6, "model.MPQ", &[1, 0, 2, 3, 4, 5, 6, 7][..]),
    ] {
        let Outcome::Present(member) = &source.receipts[index].outcome else {
            panic!("present receipt");
        };
        assert_eq!(member.winning_archive, winner);
        assert_eq!(member.bytes, payload.len());
        assert_eq!(member.sha256, format!("{:x}", Sha256::digest(payload)));
    }
    assert_eq!(source.read_work, 78);
    assert_eq!(
        source.binding_digest(),
        "13f35e84b124f49269d471292c0ea42e67ba5be1d948d01ab53b64026f4c6335"
    );
    let polls = Cell::new(0);
    source
        .verify_unchanged(&|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
    let total = polls.get();
    // Two archive directory entries, all104 candidate paths, then all seven
    // receipts in their original order, including both repeated names.
    assert_eq!(total, 139);
    for cutoff in 1..=total + 1 {
        let run = |owned: bool| {
            let calls = Cell::new(0);
            let cancel = || {
                calls.set(calls.get() + 1);
                calls.get() == cutoff
            };
            let result = if owned {
                verify_with_owned_receipts(&source, &cancel)
            } else {
                source.verify_unchanged(&cancel)
            };
            (result, calls.get())
        };
        assert_eq!(run(false), run(true));
        if cutoff <= total {
            assert_eq!(run(false).0, Err(SourceError::Cancelled));
        }
    }
    let chain = open_chain(&data, &|| false).unwrap();
    let mut work = 0;
    for expected in &source.receipts {
        let (_, actual) =
            read_lookup(&chain, &expected.name, expected.scope, &mut work, &|| false).unwrap();
        assert!(actual.matches(expected));
        let mut bad = expected.clone();
        bad.probe_count += 1;
        assert!(!actual.matches(&bad));
        bad = expected.clone();
        bad.outcome = if matches!(expected.outcome, Outcome::Absent) {
            Outcome::Dummy {
                winning_archive: "model.MPQ".into(),
                bytes: 0,
            }
        } else {
            Outcome::Absent
        };
        assert!(!actual.matches(&bad));
    }
    assert_eq!(work, source.read_work);
}

#[test]
fn unnamed_lookup_keeps_final_payload_read_after_intermediate_poll() {
    use std::cell::Cell;
    for replace_path in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        store(&data, "model.MPQ", &[entry(MAP, 1)]);
        store(&data, "patch.MPQ", &[]);
        let chain = open_chain(&data, &|| false).unwrap();
        let polls = Cell::new(0);
        let cancel = || {
            polls.set(polls.get() + 1);
            // Patch miss, model hit, pre-profile poll, then pre-payload poll.
            if polls.get() == 4 {
                let updated = archive(&[entry(MAP, 2)]);
                if replace_path {
                    let next = data.join("replacement");
                    fs::write(&next, updated).unwrap();
                    fs::rename(next, data.join("model.MPQ")).unwrap();
                } else {
                    fs::write(data.join("model.MPQ"), updated).unwrap();
                }
            }
            false
        };
        let mut work = 0;
        let (lookup, observation) =
            read_lookup(&chain, MAP, LookupScope::Chain, &mut work, &cancel).unwrap();
        assert_eq!(present(lookup).bytes()[0], 2);
        assert_eq!(polls.get(), 5);
        assert_eq!(work, 16);
        let Outcome::Present(member) = observation.outcome else {
            panic!("present observation");
        };
        assert_eq!(
            member.sha256,
            format!("{:x}", Sha256::digest([2, 0, 2, 3, 4, 5, 6, 7]))
        );
    }
}

#[test]
fn unnamed_replay_matches_owned_materialization_over_absent_grid() {
    use std::time::Instant;
    let temp = Temp::new();
    let data = temp.data();
    store(&data, "model.MPQ", &[entry(MAP, 1)]);
    let mut source = CapturedSource::open(&data, &|| false).unwrap();
    for row in 0..64 {
        for col in 0..64 {
            source
                .read(
                    &format!("World\\Maps\\Authored\\Authored_{row}_{col}.adt"),
                    &|| false,
                )
                .unwrap();
        }
    }
    assert_eq!(source.receipts.len(), 4096);
    assert!(source
        .receipts
        .iter()
        .all(|r| matches!(r.outcome, Outcome::Absent)));
    assert_eq!(source.read_work, 0);
    let mut owned = Vec::new();
    let mut unnamed = Vec::new();
    // Same-process bounded diagnostic, alternating order. No timing threshold:
    // runner observations are not an end-to-end Route performance guarantee.
    for sample in 0..9 {
        for materialize in [sample % 2 == 0, sample % 2 != 0] {
            let start = Instant::now();
            if materialize {
                verify_with_owned_receipts(&source, &|| false).unwrap();
            } else {
                source.verify_unchanged(&|| false).unwrap();
            }
            let elapsed = start.elapsed();
            if materialize {
                owned.push(elapsed);
            } else {
                unnamed.push(elapsed);
            }
        }
    }
    owned.sort();
    unnamed.sort();
    eprintln!("vmap absent-grid replay: owned_receipt_median_ns={} unnamed_observation_median_ns={} lookups=4096 samples=9", owned[4].as_nanos(), unnamed[4].as_nanos());
}
