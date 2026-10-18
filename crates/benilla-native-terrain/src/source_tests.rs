// SPDX-License-Identifier: GPL-3.0-or-later
//! Newly authored MPQ fixture bytes only; no client install, assets, or rows.

use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

// The same test-only table encoder used by benilla-native-dbc/extract_chain.rs.
#[path = "../../benilla-mpq/src/crypto.rs"]
mod crypto;

const MAP: &str = "DBFilesClient\\Map.dbc";
const AREA: &str = "DBFilesClient\\AreaTable.dbc";
const LIQUID: &str = "DBFilesClient\\LiquidType.dbc";
const WDT: &str = "World\\Maps\\Authored\\Authored.wdt";

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "native-terrain-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
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

fn entry(name: &str, marker: u8) -> Entry {
    raw_entry(name, vec![marker, 0, 2, 3, 4, 5, 6, 7])
}

fn raw_entry(name: &str, bytes: Vec<u8>) -> Entry {
    Entry {
        name: name.to_owned(),
        declared: bytes.len() as u32,
        bytes,
        flags: 0x8000_0000,
    }
}

fn archive(entries: &[Entry]) -> Vec<u8> {
    archive_version(entries, 0)
}

fn archive_version(entries: &[Entry], version: u16) -> Vec<u8> {
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

fn assert_error<T>(result: Result<T>, expected: &str) {
    let error = result.err().expect("operation must fail closed");
    assert!(format!("{error:#}").contains(expected), "{error:#}");
}

#[test]
fn capture_reads_real_patch_winner_and_preserves_every_source_byte() {
    let temp = Temp::new();
    let data = temp.data();
    let originals = [
        ("dbc.MPQ", archive(&[entry(MAP, 1), entry(AREA, 1)])),
        ("patch.MPQ", archive(&[entry(MAP, 2)])),
        ("patch-2.MPQ", archive(&[entry(MAP, 3)])),
        ("patch-10.MPQ", archive(&[entry(MAP, 9)])),
    ];
    for (name, bytes) in &originals {
        fs::write(data.join(name), bytes).unwrap();
    }
    let source = CapturedSource::capture(&data, &[MAP, AREA]).unwrap();
    assert_eq!(source.bytes(MAP).unwrap(), entry(MAP, 3).bytes);
    assert_eq!(source.members()[1].name, MAP);
    assert_eq!(source.members()[1].winning_archive, "patch-2.MPQ");
    assert_eq!(source.members()[1].bytes, 8);
    assert_eq!(
        source.members()[1].sha256,
        format!("{:x}", Sha256::digest(&entry(MAP, 3).bytes))
    );
    source.verify_unchanged().unwrap();
    assert_error(source.bytes(LIQUID), "source.not_captured");
    assert_eq!(fs::read_dir(&data).unwrap().count(), originals.len());
    for (name, bytes) in originals {
        assert_eq!(fs::read(data.join(name)).unwrap(), bytes);
    }
}

#[test]
fn header_policy_versions_the_consumed_buffer_epoch() {
    let temp = Temp::new();
    let data = temp.data();
    let bytes = entry(MAP, 1).bytes;
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let captured = CapturedSource::capture(&data, &[MAP]).unwrap();
    let digest = |policy: &str| {
        let mut binding = Sha256::new();
        binding.update(b"benilla-native-terrain-source-v1\0");
        hash_field(&mut binding, policy.as_bytes());
        hash_field(&mut binding, crate::SOURCE_PIN.as_bytes());
        binding.update(1u64.to_le_bytes());
        hash_field(&mut binding, MAP.as_bytes());
        hash_field(&mut binding, b"dbc.MPQ");
        hash_field(&mut binding, &bytes);
        format!("{:x}", binding.finalize())
    };
    assert_eq!(
        SOURCE_POLICY,
        "benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v3"
    );
    assert_eq!(captured.binding_digest(), digest(SOURCE_POLICY));
    assert_ne!(
        captured.binding_digest(),
        digest("benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v2")
    );
}

#[test]
fn immutable_capture_detects_same_winner_same_length_tail_mutation() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let source = CapturedSource::capture(&data, &[MAP]).unwrap();
    let digest = source.binding_digest().to_owned();
    let mut changed = entry(MAP, 1);
    *changed.bytes.last_mut().unwrap() = 77;
    fs::write(data.join("dbc.MPQ"), archive(&[changed])).unwrap();
    assert_eq!(source.bytes(MAP).unwrap(), entry(MAP, 1).bytes);
    assert_eq!(source.binding_digest(), digest);
    assert_error(source.verify_unchanged(), "source.bytes_changed");
    let fresh = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert_ne!(fresh.binding_digest(), digest);
}

#[test]
fn equal_bytes_from_a_new_winner_are_a_different_capture() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let source = CapturedSource::capture(&data, &[MAP]).unwrap();
    fs::write(data.join("patch.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    assert_error(source.verify_unchanged(), "source.winner_changed");
    let fresh = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert_eq!(source.bytes(MAP).unwrap(), fresh.bytes(MAP).unwrap());
    assert_ne!(source.binding_digest(), fresh.binding_digest());
    assert_error(fresh.verify_contains(&source), "source.discovery_changed");
}

#[test]
fn deleted_and_missing_members_never_fall_back() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let source = CapturedSource::capture(&data, &[MAP]).unwrap();
    let mut deleted = raw_entry(MAP, Vec::new());
    deleted.flags = 0x8200_0000;
    fs::write(data.join("patch.MPQ"), archive(&[deleted])).unwrap();
    assert_error(CapturedSource::capture(&data, &[MAP]), "tombstoned");
    assert_error(source.verify_unchanged(), "tombstoned");
    fs::remove_file(data.join("patch.MPQ")).unwrap();
    assert_error(
        CapturedSource::capture(&data, &[AREA]),
        "source.missing_or_deleted",
    );
    fs::write(data.join("dbc.MPQ"), archive(&[entry(AREA, 1)])).unwrap();
    assert_error(source.verify_unchanged(), "source.missing_or_deleted");
}

#[test]
fn corrupt_winner_does_not_use_valid_lower_bytes() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let mut corrupt = entry(MAP, 2);
    corrupt.declared += 1;
    fs::write(data.join("patch.MPQ"), archive(&[corrupt])).unwrap();
    assert_error(CapturedSource::capture(&data, &[MAP]), "source.read");
}

#[test]
fn all_member_sizes_are_budgeted_before_any_payload_is_read() {
    let temp = Temp::new();
    let data = temp.data();
    let mut oversized = entry(MAP, 1);
    oversized.declared = MAX_MEMBER_BYTES as u32 + 1;
    fs::write(data.join("dbc.MPQ"), archive(&[oversized])).unwrap();
    assert_error(
        CapturedSource::capture(&data, &[MAP]),
        "source.member_byte_limit",
    );
    let names = [MAP, AREA, LIQUID, WDT];
    let entries: Vec<_> = names
        .iter()
        .map(|name| {
            let mut item = entry(name, 1);
            item.declared = MAX_MEMBER_BYTES as u32;
            item
        })
        .collect();
    fs::write(data.join("dbc.MPQ"), archive(&entries)).unwrap();
    // A read would fail because no payload actually has its declared size.
    assert_error(
        CapturedSource::capture(&data, &names),
        "source.aggregate_byte_limit",
    );
}

#[test]
fn allowlist_rejects_aliases_traversal_duplicates_and_unbounded_selection() {
    for name in [
        "dbfilesclient\\Map.dbc",
        "DBFilesClient/Map.dbc",
        "DBFilesClient\\Spell.dbc",
        "World\\Maps\\..\\...wdt",
        "World\\Maps\\Authored\\Other.wdt",
        "World\\Maps\\Authored\\Authored_64_0.adt",
        "World\\Maps\\Authored\\Authored_00_0.adt",
        "World\\Maps\\Authored\\Authored_1_2_3.adt",
        "World\\Maps\\Authored\\Authored_0_0.adt/extra",
        "World\\Maps\\Authored\\Authored_+1_0.adt",
    ] {
        assert!(selected_names(&[name]).is_err(), "{name}");
    }
    assert_error(selected_names(&[] as &[&str]), "source.member_count_limit");
    assert_error(selected_names(&[MAP; 14]), "source.member_count_limit");
    assert_error(selected_names(&[MAP, MAP]), "source.duplicate_member");
    assert_error(
        selected_names(&[WDT, "World\\Maps\\Other\\Other_0_0.adt"]),
        "source.multiple_maps",
    );
    let names: Vec<_> = (0..10)
        .map(|column| format!("World\\Maps\\Authored\\Authored_{column}_0.adt"))
        .collect();
    assert_error(selected_names(&names), "source.adt_count_limit");
}

#[test]
fn captures_exactly_the_full_thirteen_member_envelope_in_stable_order() {
    let temp = Temp::new();
    let data = temp.data();
    let mut names: Vec<_> = [MAP, AREA, LIQUID, WDT]
        .into_iter()
        .map(str::to_owned)
        .collect();
    names.extend((0..9).map(|column| format!("World\\Maps\\Authored\\Authored_{column}_63.adt")));
    let entries: Vec<_> = names.iter().map(|name| entry(name, 1)).collect();
    fs::write(data.join("terrain.MPQ"), archive(&entries)).unwrap();
    let first = CapturedSource::capture(&data, &names).unwrap();
    assert_eq!(first.members().len(), MAX_CAPTURE_MEMBERS);
    names.reverse();
    let second = CapturedSource::capture(&data, &names).unwrap();
    assert_eq!(first.binding_digest(), second.binding_digest());
    second.verify_contains(&first).unwrap();
}

#[test]
fn staged_discovery_compares_exact_subset_and_directory() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(
        data.join("dbc.MPQ"),
        archive(&[entry(MAP, 1), entry(AREA, 2)]),
    )
    .unwrap();
    let first = CapturedSource::capture(&data, &[MAP]).unwrap();
    let complete = CapturedSource::capture(&data, &[MAP, AREA]).unwrap();
    complete.verify_contains(&first).unwrap();
    assert_error(first.verify_contains(&complete), "source.discovery_missing");
    fs::write(
        data.join("dbc.MPQ"),
        archive(&[entry(MAP, 3), entry(AREA, 2)]),
    )
    .unwrap();
    let changed = CapturedSource::capture(&data, &[MAP, AREA]).unwrap();
    assert_error(changed.verify_contains(&first), "source.discovery_changed");
    let other = Temp::new();
    let other_data = other.data();
    fs::write(other_data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let other_source = CapturedSource::capture(&other_data, &[MAP]).unwrap();
    assert_error(
        complete.verify_contains(&other_source),
        "source.discovery_directory_changed",
    );
}

#[test]
fn source_profile_skips_absent_fixed_archives_and_rejects_dummy_hits() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("patch-2.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    CapturedSource::capture(&data, &[MAP])
        .unwrap()
        .verify_unchanged()
        .unwrap();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    for bytes in [vec![], vec![1]] {
        fs::write(data.join("patch-2.MPQ"), archive(&[raw_entry(MAP, bytes)])).unwrap();
        assert_error(CapturedSource::capture(&data, &[MAP]), "dummy member");
    }
}

#[test]
fn source_profile_rejects_app_only_winners_even_with_identical_bytes() {
    for winner in ["patch-3.MPQ", "speech2.MPQ", "texture.MPQ"] {
        let temp = Temp::new();
        let data = temp.data();
        fs::write(data.join(winner), archive(&[entry(MAP, 1)])).unwrap();
        // texture precedes dbc in the app chain, so this case intentionally has
        // no source-profile copy; later app-only archives also shadow equal bytes.
        if winner != "texture.MPQ" {
            fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
        }
        assert_error(CapturedSource::capture(&data, &[MAP]), "source.ad_");
    }
}

#[test]
fn source_profile_rejects_case_aliases_of_fixed_archive_names() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("DBC.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    assert_error(
        CapturedSource::capture(&data, &[MAP]),
        "source.ad_archive_case_ambiguity",
    );
}

fn prefixed_archive(prefix: usize, bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![0; prefix];
    result.extend(bytes);
    result
}

#[test]
fn source_profile_accepts_bounded_512_scan_and_normalized_legacy_header_size() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("dbc.MPQ");
    for prefix in [0, 512, 1024, MAX_HEADER_SCAN_BYTES as usize - 512] {
        for size in [0u32, 1, 31, 32, 33, u32::MAX] {
            let mut original = archive(&[entry(MAP, 1)]);
            original[4..8].copy_from_slice(&size.to_le_bytes());
            let prefixed = prefixed_archive(prefix, &original);
            fs::write(&path, &prefixed).unwrap();
            let expected = (prefix as u64, original[..32].try_into().unwrap());
            assert_eq!(
                verify_ad_archive_header(&path).unwrap(),
                AdHeaderSnapshot {
                    base: expected,
                    extension: None
                }
            );
            let chain = open_chain(&data).unwrap();
            assert_eq!(chain.archive_for(MAP).unwrap().header_snapshot(), expected);
            let capture = CapturedSource::capture(&data, &[MAP]).unwrap();
            assert_eq!(capture.bytes(MAP).unwrap(), entry(MAP, 1).bytes);
            capture.verify_unchanged().unwrap();
            assert_eq!(fs::read(&path).unwrap(), prefixed);
        }
    }
}

#[test]
fn version_one_accepts_normalized_sizes_and_retains_complete_decoder_snapshot() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("patch.MPQ");
    for prefix in [0, 512, MAX_HEADER_SCAN_BYTES as usize - 512] {
        for size in [0u32, 1, 32, 43, 44, 45, u32::MAX] {
            let mut original = archive_version(&[entry(MAP, 1)], 1);
            original[4..8].copy_from_slice(&size.to_le_bytes());
            let prefixed = prefixed_archive(prefix, &original);
            fs::write(&path, &prefixed).unwrap();
            let expected = verify_ad_archive_header(&path).unwrap();
            assert_eq!(
                expected.base,
                (prefix as u64, original[..32].try_into().unwrap())
            );
            assert_eq!(expected.extension, Some([0; 12]));
            let chain = open_chain(&data).unwrap();
            let decoded = chain.archive_for(MAP).unwrap();
            assert_eq!(decoded.header_snapshot(), expected.base);
            assert_eq!(decoded.header_extension_snapshot(), expected.extension);
            let capture = CapturedSource::capture(&data, &[MAP]).unwrap();
            assert_eq!(capture.bytes(MAP).unwrap(), entry(MAP, 1).bytes);
            capture.verify_unchanged().unwrap();
            assert_eq!(fs::read(&path).unwrap(), prefixed);
        }
    }
}

#[test]
fn mixed_raw_versions_accept_the_reported_patch_header_shapes_and_lengths() {
    let temp = Temp::new();
    let data = temp.data();
    for (name, marker) in [("model.MPQ", 1), ("dbc.MPQ", 2), ("terrain.MPQ", 3)] {
        fs::write(data.join(name), archive(&[entry(MAP, marker)])).unwrap();
    }
    // Sparse authored files exercise the exact reported lengths without client content.
    for (name, marker, len) in [
        ("patch.MPQ", 4, 1_909_274_747u32),
        ("patch-2.MPQ", 5, 9_095_701),
    ] {
        let path = data.join(name);
        let mut bytes = archive_version(&[entry(MAP, marker)], 1);
        bytes[8..12].copy_from_slice(&len.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(u64::from(len))
            .unwrap();
        let snapshot = verify_ad_archive_header(&path).unwrap();
        assert_eq!(snapshot.base.0, 0);
        assert_eq!(
            u32::from_le_bytes(snapshot.base.1[4..8].try_into().unwrap()),
            44
        );
        assert_eq!(
            u16::from_le_bytes(snapshot.base.1[12..14].try_into().unwrap()),
            1
        );
        assert_eq!(
            u16::from_le_bytes(snapshot.base.1[14..16].try_into().unwrap()),
            3
        );
        assert_eq!(snapshot.extension, Some([0; 12]));
    }
    let capture = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert_eq!(capture.bytes(MAP).unwrap(), entry(MAP, 5).bytes);
    assert_eq!(capture.members()[0].winning_archive, "patch-2.MPQ");
    capture.verify_unchanged().unwrap();
    assert_eq!(
        fs::metadata(data.join("patch.MPQ")).unwrap().len(),
        1_909_274_747
    );
    assert_eq!(
        fs::metadata(data.join("patch-2.MPQ")).unwrap().len(),
        9_095_701
    );
    assert_eq!(fs::read_dir(&data).unwrap().count(), 5);
}

#[test]
fn version_one_refuses_each_truncated_or_nonzero_extension_and_table_start() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("patch.MPQ");
    let original = archive_version(&[entry(MAP, 1)], 1);
    for prefix in [0, 512] {
        for present in 0..12 {
            fs::write(&path, prefixed_archive(prefix, &original[..32 + present])).unwrap();
            assert_error(
                verify_ad_archive_header(&path),
                "source.ad_header_truncated",
            );
            assert_error(
                CapturedSource::capture(&data, &[MAP]),
                "source.ad_header_truncated",
            );
        }
        // Every extension byte, including all eight bytes of the extended-table position.
        for at in 32..44 {
            let mut changed = original.clone();
            changed[at] = 1;
            fs::write(&path, prefixed_archive(prefix, &changed)).unwrap();
            assert_error(
                verify_ad_archive_header(&path),
                "source.ad_header_extension",
            );
            assert_error(
                CapturedSource::capture(&data, &[MAP]),
                "source.ad_header_extension",
            );
        }
        for start in 32u32..44 {
            for field in [16, 20] {
                let mut changed = original.clone();
                changed[field..field + 4].copy_from_slice(&start.to_le_bytes());
                fs::write(&path, prefixed_archive(prefix, &changed)).unwrap();
                assert_error(verify_ad_archive_header(&path), "source.ad_index_spans");
                assert_error(
                    CapturedSource::capture(&data, &[MAP]),
                    "source.ad_index_spans",
                );
            }
        }
    }
}

#[test]
fn version_one_capture_and_fresh_reopen_refuse_extension_only_source_change() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("patch.MPQ");
    let original = archive_version(&[entry(MAP, 1)], 1);
    fs::write(&path, &original).unwrap();
    let capture = CapturedSource::capture(&data, &[MAP]).unwrap();
    let chain = open_chain(&data).unwrap();
    let decoded = chain.archive_for(MAP).unwrap();
    let expected = verify_ad_archive_header(&path).unwrap();
    for at in 32..44 {
        let mut changed = original.clone();
        changed[at] = 1;
        fs::write(&path, changed).unwrap();
        assert_eq!(decoded.header_snapshot(), expected.base);
        assert_eq!(decoded.header_extension_snapshot(), expected.extension);
        assert_eq!(capture.bytes(MAP).unwrap(), entry(MAP, 1).bytes);
        assert_error(capture.verify_unchanged(), "source.ad_header_extension");
        assert_error(
            CapturedSource::capture(&data, &[MAP]),
            "source.ad_header_extension",
        );
    }
}

#[test]
fn source_profile_preserves_version_size_sector_and_index_bounds_after_prefix() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("dbc.MPQ");
    for version in [0, 1] {
        let original = archive_version(&[entry(MAP, 1)], version);
        for prefix in [0, 512] {
            for (offset, value, error) in [
                (8, 0u32, "source.ad_archive_size"),
                (8, original.len() as u32 + 512, "source.ad_archive_size"),
                (12, 2, "source.ad_header_version"),
                (12, u16::MAX as u32, "source.ad_header_version"),
                (12, (8 << 16) | u32::from(version), "source.ad_sector_size"),
                (24, 63, "source.ad_hash_count"),
                (24, 0, "source.ad_hash_count"),
                (24, 1 << 23, "source.ad_index_budget"),
                (20, 32, "source.ad_index_spans"),
                (16, 31, "source.ad_index_spans"),
                (20, original.len() as u32, "source.ad_index_spans"),
            ] {
                let mut changed = original.clone();
                changed[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                fs::write(&path, prefixed_archive(prefix, &changed)).unwrap();
                assert_error(verify_ad_archive_header(&path), error);
                assert_error(CapturedSource::capture(&data, &[MAP]), error);
            }
        }
    }
}

#[test]
fn source_profile_reports_missing_truncated_and_out_of_budget_headers() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("dbc.MPQ");
    for (bytes, error) in [
        (vec![0; 1024], "source.ad_header_signature"),
        (
            prefixed_archive(512, b"MPQ\x1a"),
            "source.ad_header_truncated",
        ),
        (
            prefixed_archive(MAX_HEADER_SCAN_BYTES as usize, &archive(&[entry(MAP, 1)])),
            "source.ad_header_scan_limit",
        ),
    ] {
        fs::write(&path, bytes).unwrap();
        assert_error(verify_ad_archive_header(&path), error);
        assert_error(CapturedSource::capture(&data, &[MAP]), error);
    }
}

#[test]
fn source_profile_does_not_skip_first_unsupported_header_for_later_valid_one() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("dbc.MPQ");
    let mut first = archive(&[entry(MAP, 1)]);
    first[12..14].copy_from_slice(&2u16.to_le_bytes());
    let mut bytes = prefixed_archive(512, &first);
    bytes.resize(2048, 0);
    bytes.extend(archive(&[entry(MAP, 1)]));
    fs::write(&path, bytes).unwrap();
    assert_error(verify_ad_archive_header(&path), "source.ad_header_version");
    assert_error(
        CapturedSource::capture(&data, &[MAP]),
        "source.ad_header_version",
    );
}

#[test]
fn source_profile_requires_reader_and_extractor_to_select_the_same_header() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("dbc.MPQ");
    let original = archive(&[entry(MAP, 1)]);
    let mut unaligned = prefixed_archive(16, &original);
    unaligned[..4].copy_from_slice(b"MPQ\x1b");
    unaligned[8..12].copy_from_slice(&16u32.to_le_bytes());
    fs::write(&path, unaligned).unwrap();
    // Normal rendering supports this pointer, but pinned libmpq does not.
    assert_eq!(
        Chain::open(&data).unwrap().read(MAP).unwrap(),
        entry(MAP, 1).bytes
    );
    assert_error(
        CapturedSource::capture(&data, &[MAP]),
        "source.ad_header_signature",
    );

    let mut aligned = prefixed_archive(512, &original);
    aligned[..4].copy_from_slice(b"MPQ\x1b");
    aligned[8..12].copy_from_slice(&512u32.to_le_bytes());
    fs::write(&path, &aligned).unwrap();
    CapturedSource::capture(&data, &[MAP])
        .unwrap()
        .verify_unchanged()
        .unwrap();

    // Equal member bytes do not excuse different selected archive headers.
    aligned.resize(2048, 0);
    aligned.extend(&original);
    aligned[8..12].copy_from_slice(&2048u32.to_le_bytes());
    let source_size = (aligned.len() - 512) as u32;
    aligned[520..524].copy_from_slice(&source_size.to_le_bytes());
    fs::write(&path, aligned).unwrap();
    assert_eq!(verify_ad_archive_header(&path).unwrap().base.0, 512);
    assert_eq!(
        Chain::open(&data)
            .unwrap()
            .archive_for(MAP)
            .unwrap()
            .header_snapshot()
            .0,
        2048
    );
    assert_error(
        CapturedSource::capture(&data, &[MAP]),
        "source.ad_header_selection",
    );
}

#[test]
fn prefixed_capture_still_detects_changed_bytes_and_equal_bytes_from_new_winner() {
    let temp = Temp::new();
    let data = temp.data();
    let path = data.join("dbc.MPQ");
    let original = prefixed_archive(512, &archive(&[entry(MAP, 1)]));
    fs::write(&path, &original).unwrap();
    let capture = CapturedSource::capture(&data, &[MAP]).unwrap();
    let mut changed = original.clone();
    *changed.last_mut().unwrap() ^= 1;
    fs::write(&path, changed).unwrap();
    assert_error(capture.verify_unchanged(), "source.bytes_changed");
    fs::write(&path, &original).unwrap();
    fs::write(data.join("patch.MPQ"), &original).unwrap();
    assert_error(capture.verify_unchanged(), "source.winner_changed");
    fs::remove_file(data.join("patch.MPQ")).unwrap();
    let mut changed = original;
    changed[524..526].copy_from_slice(&2u16.to_le_bytes());
    fs::write(&path, changed).unwrap();
    assert_error(capture.verify_unchanged(), "source.ad_header_version");
}

#[test]
fn compatibility_check_distinguishes_absence_from_hidden_malformed_or_deleted_hits() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let chain = open_chain(&data).unwrap();
    assert!(chain.verify_classic_ad_member(MAP).unwrap());
    assert!(!chain.verify_classic_ad_member(AREA).unwrap());
    let mut malformed = archive(&[entry(MAP, 2)]);
    let mut words: Vec<_> = malformed[32..32 + 64 * 16]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word))
        .collect();
    let key = crypto::hash_string("(hash table)", crypto::hash_type::FILE_KEY);
    crypto::decrypt_block(&mut words, key);
    let slot = (crypto::hash_string(MAP, crypto::hash_type::TABLE_OFFSET) & 63) as usize;
    words[slot * 4 + 3] = 0xffff_fffe;
    crypto::encrypt_block(&mut words, key);
    for (index, word) in words.into_iter().enumerate() {
        malformed[32 + index * 4..36 + index * 4].copy_from_slice(&word.to_le_bytes());
    }
    fs::write(data.join("patch.MPQ"), malformed).unwrap();
    let chain = open_chain(&data).unwrap();
    assert_eq!(chain.read(MAP).unwrap(), entry(MAP, 1).bytes);
    assert_error(
        chain.verify_classic_ad_member(MAP),
        "matching invalid hash index",
    );
    assert_error(
        CapturedSource::capture(&data, &[MAP]),
        "matching invalid hash index",
    );
    let mut deleted = raw_entry(MAP, Vec::new());
    deleted.flags = 0x8200_0000;
    fs::write(data.join("patch.MPQ"), archive(&[deleted])).unwrap();
    assert_error(
        open_chain(&data).unwrap().verify_classic_ad_member(MAP),
        "unsupported member flags",
    );
}

#[test]
fn output_parent_cannot_be_anywhere_in_source_installation() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let source = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert_error(
        source.check_output_parent(&data.canonicalize().unwrap()),
        "output.inside_source_installation",
    );
    assert_error(
        source.check_output_parent(&temp.0.canonicalize().unwrap()),
        "output.inside_source_installation",
    );
    let other = Temp::new();
    source
        .check_output_parent(&other.0.canonicalize().unwrap())
        .unwrap();
}

const HISTORY_ADT: &str = "World\\Maps\\Earlier\\Earlier_63_63.adt";
const HISTORY_WDT: &str = "World\\Maps\\Earlier\\Earlier.wdt";

fn source_receipts(
    source: &CapturedSource,
) -> (Vec<SourceMember>, Vec<AbsentSourceMember>, String) {
    (
        source.members().to_vec(),
        source.absent_members().to_vec(),
        source.binding_digest().to_owned(),
    )
}

#[test]
fn history_captures_optional_members_across_maps_and_preserves_selected_envelope() {
    let temp = Temp::new();
    let data = temp.data();
    let entries = [
        entry(MAP, 1),
        entry(WDT, 2),
        entry(HISTORY_WDT, 3),
        entry(HISTORY_ADT, 4),
    ];
    fs::write(data.join("terrain.MPQ"), archive(&entries)).unwrap();
    let mut source = CapturedSource::capture(&data, &[MAP, WDT]).unwrap();
    let selected_epoch = source.binding_digest().to_owned();
    assert!(source.capture_history_member(HISTORY_ADT).unwrap());
    assert!(source.capture_history_member(HISTORY_WDT).unwrap());
    assert_eq!(
        source.bytes(HISTORY_ADT).unwrap(),
        entry(HISTORY_ADT, 4).bytes
    );
    assert_eq!(
        source.bytes(HISTORY_WDT).unwrap(),
        entry(HISTORY_WDT, 3).bytes
    );
    assert_eq!(source.members().len(), 4);
    assert!(source.absent_members().is_empty());
    assert_ne!(selected_epoch, source.binding_digest());
    assert_error(
        CapturedSource::capture(&data, &[WDT, HISTORY_WDT]),
        "source.multiple_maps",
    );
    source.verify_unchanged().unwrap();
}

#[test]
fn history_absence_receipts_are_explicit_canonical_and_epoch_bound() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(
        data.join("terrain.MPQ"),
        archive(&[entry(MAP, 1), entry(WDT, 2)]),
    )
    .unwrap();
    let mut forward = CapturedSource::capture(&data, &[MAP]).unwrap();
    let initial = forward.binding_digest().to_owned();
    assert!(!forward.capture_history_member(HISTORY_ADT).unwrap());
    let one_absence = forward.binding_digest().to_owned();
    assert_ne!(initial, one_absence);
    assert!(!forward.capture_history_member(HISTORY_WDT).unwrap());
    assert_ne!(one_absence, forward.binding_digest());
    assert!(forward.capture_history_member(WDT).unwrap());
    assert_eq!(
        forward.absent_members(),
        &[
            AbsentSourceMember {
                name: HISTORY_WDT.to_owned()
            },
            AbsentSourceMember {
                name: HISTORY_ADT.to_owned()
            },
        ]
    );
    assert_error(forward.bytes(HISTORY_ADT), "source.not_captured");
    let receipts = source_receipts(&forward);
    assert!(!forward.capture_history_member(HISTORY_ADT).unwrap());
    assert!(forward.capture_history_member(WDT).unwrap());
    assert_eq!(source_receipts(&forward), receipts);
    assert_eq!(forward.history_names.len(), 3);
    let mut reverse = CapturedSource::capture(&data, &[MAP, WDT]).unwrap();
    assert!(!reverse.capture_history_member(HISTORY_WDT).unwrap());
    assert!(!reverse.capture_history_member(HISTORY_ADT).unwrap());
    assert_eq!(source_receipts(&reverse), receipts);
    reverse.verify_contains(&forward).unwrap();
    forward.verify_contains(&reverse).unwrap();
    forward.verify_unchanged().unwrap();
}

#[test]
fn history_existing_selected_member_reuses_bytes_without_changing_epoch() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(
        data.join("terrain.MPQ"),
        archive(&[entry(MAP, 1), entry(WDT, 2)]),
    )
    .unwrap();
    let mut source = CapturedSource::capture(&data, &[MAP, WDT]).unwrap();
    let receipts = source_receipts(&source);
    assert!(source.capture_history_member(WDT).unwrap());
    assert_eq!(source_receipts(&source), receipts);
    assert_eq!(source.history_names.len(), 1);
}

#[test]
fn first_history_extension_rechecks_original_exact_proofs_before_mutating() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let mut source = CapturedSource::capture(&data, &[MAP]).unwrap();
    let receipts = source_receipts(&source);
    fs::write(
        data.join("dbc.MPQ"),
        archive(&[entry(MAP, 2), entry(HISTORY_ADT, 3)]),
    )
    .unwrap();
    assert_error(
        source.capture_history_member(HISTORY_ADT),
        "source.bytes_changed",
    );
    assert_eq!(source_receipts(&source), receipts);
    assert!(source.history_names.is_empty());
    fs::write(
        data.join("dbc.MPQ"),
        archive(&[entry(MAP, 1), entry(HISTORY_ADT, 3)]),
    )
    .unwrap();
    assert!(source.capture_history_member(HISTORY_ADT).unwrap());
}

#[test]
fn history_absence_becoming_present_in_either_resolution_fails_fresh_recheck() {
    for name in [HISTORY_WDT, HISTORY_ADT] {
        for winner in ["terrain.MPQ", "patch-3.MPQ"] {
            let temp = Temp::new();
            let data = temp.data();
            fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
            let mut source = CapturedSource::capture(&data, &[MAP]).unwrap();
            assert!(!source.capture_history_member(name).unwrap());
            let receipts = source_receipts(&source);
            fs::write(data.join(winner), archive(&[entry(name, 2)])).unwrap();
            assert_error(source.verify_unchanged(), "source.absence_changed");
            assert_eq!(source_receipts(&source), receipts);
        }
    }
}

#[test]
fn history_positive_byte_and_winner_changes_fail_fresh_recheck() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let original = archive(&[entry(HISTORY_ADT, 2)]);
    fs::write(data.join("terrain.MPQ"), &original).unwrap();
    let mut source = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert!(source.capture_history_member(HISTORY_ADT).unwrap());
    let mut changed = entry(HISTORY_ADT, 2);
    *changed.bytes.last_mut().unwrap() ^= 1;
    fs::write(data.join("terrain.MPQ"), archive(&[changed])).unwrap();
    assert_error(source.verify_unchanged(), "source.bytes_changed");
    fs::write(data.join("terrain.MPQ"), &original).unwrap();
    fs::write(data.join("patch.MPQ"), original).unwrap();
    assert_error(source.verify_unchanged(), "source.winner_changed");
}

#[test]
fn history_dummy_deleted_corrupt_and_app_only_hits_are_errors_not_absence() {
    for (winner, item, expected) in [
        (
            "terrain.MPQ",
            raw_entry(HISTORY_ADT, vec![]),
            "dummy member",
        ),
        (
            "terrain.MPQ",
            raw_entry(HISTORY_ADT, vec![0]),
            "dummy member",
        ),
        (
            "terrain.MPQ",
            Entry {
                flags: 0x8200_0000,
                ..raw_entry(HISTORY_ADT, vec![])
            },
            "tombstoned",
        ),
        (
            "terrain.MPQ",
            Entry {
                declared: 9,
                ..entry(HISTORY_ADT, 2)
            },
            "source.history_member_profile",
        ),
        (
            "patch-3.MPQ",
            entry(HISTORY_ADT, 2),
            "source.ad_missing_member",
        ),
    ] {
        let temp = Temp::new();
        let data = temp.data();
        fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
        let mut source = CapturedSource::capture(&data, &[MAP]).unwrap();
        // Establish history before introducing the candidate failure, to exercise
        // the extension's own checks rather than only its first-call recheck.
        assert!(!source.capture_history_member(HISTORY_WDT).unwrap());
        let receipts = source_receipts(&source);
        let lookups = source.history_names.clone();
        fs::write(data.join(winner), archive(&[item])).unwrap();
        assert_error(source.capture_history_member(HISTORY_ADT), expected);
        assert_eq!(source_receipts(&source), receipts);
        assert_eq!(source.history_names, lookups);
    }
}

#[test]
fn history_equal_bytes_do_not_excuse_resolver_winner_disagreement() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let mut source = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert!(!source.capture_history_member(HISTORY_WDT).unwrap());
    for name in ["terrain.MPQ", "patch-3.MPQ"] {
        fs::write(data.join(name), archive(&[entry(HISTORY_ADT, 2)])).unwrap();
    }
    assert_error(
        source.capture_history_member(HISTORY_ADT),
        "source.ad_winner_mismatch",
    );
    assert_error(source.bytes(HISTORY_ADT), "source.not_captured");
}

#[test]
fn history_rejects_unsupported_headers_and_malformed_hash_matches_without_receipts() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let mut source = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert!(!source.capture_history_member(HISTORY_WDT).unwrap());
    let receipts = source_receipts(&source);
    let mut unsupported = archive(&[entry(HISTORY_ADT, 2)]);
    unsupported[12..14].copy_from_slice(&2u16.to_le_bytes());
    fs::write(data.join("terrain.MPQ"), unsupported).unwrap();
    assert_error(
        source.capture_history_member(HISTORY_ADT),
        "source.ad_header_version",
    );
    assert_eq!(source_receipts(&source), receipts);
    fs::remove_file(data.join("terrain.MPQ")).unwrap();
    let mut malformed = archive(&[entry(HISTORY_ADT, 2)]);
    let mut words: Vec<_> = malformed[32..32 + 64 * 16]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word))
        .collect();
    let key = crypto::hash_string("(hash table)", crypto::hash_type::FILE_KEY);
    crypto::decrypt_block(&mut words, key);
    let slot = (crypto::hash_string(HISTORY_ADT, crypto::hash_type::TABLE_OFFSET) & 63) as usize;
    words[slot * 4 + 3] = 0xffff_fffe;
    crypto::encrypt_block(&mut words, key);
    for (index, word) in words.into_iter().enumerate() {
        malformed[32 + index * 4..36 + index * 4].copy_from_slice(&word.to_le_bytes());
    }
    // Even an app-only malformed match cannot be silently classified as absent.
    fs::write(data.join("patch-3.MPQ"), malformed).unwrap();
    assert_error(
        source.capture_history_member(HISTORY_ADT),
        "matching invalid hash index",
    );
    assert_eq!(source_receipts(&source), receipts);
}

#[test]
fn history_member_names_retain_safe_canonical_terrain_only_envelope() {
    for name in [
        MAP,
        AREA,
        LIQUID,
        "World\\Maps\\..\\...wdt",
        "World\\Maps\\Earlier\\Earlier_00_1.adt",
        "World\\Maps\\Earlier\\Earlier_64_0.adt",
        "World/Maps/Earlier/Earlier.wdt",
    ] {
        assert!(history_name(name).is_err(), "{name}");
    }
    for name in [WDT, HISTORY_WDT, HISTORY_ADT] {
        history_name(name).unwrap();
    }
}

#[test]
fn history_byte_budget_includes_original_and_all_predecessor_bytes() {
    let member = |bytes| SourceMember {
        name: WDT.to_owned(),
        winning_archive: "terrain.MPQ".to_owned(),
        sha256: String::new(),
        bytes,
    };
    let members = vec![
        member(MAX_MEMBER_BYTES),
        member(MAX_MEMBER_BYTES),
        member(MAX_MEMBER_BYTES - 8),
    ];
    check_history_byte_budget(&members, 8).unwrap();
    assert_error(
        check_history_byte_budget(&members, 9),
        "source.aggregate_byte_limit",
    );
    assert_error(
        check_history_byte_budget(&[member(usize::MAX), member(1)], 0),
        "source.aggregate_byte_limit",
    );
}

#[test]
fn history_distinct_lookup_limit_includes_absences_and_never_truncates() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let mut source = CapturedSource::capture(&data, &[MAP]).unwrap();
    for y in 0..64 {
        for x in 0..64 {
            assert!(!source
                .capture_history_member(&format!("World\\Maps\\Earlier\\Earlier_{x}_{y}.adt"))
                .unwrap());
        }
    }
    assert_eq!(source.history_names.len(), MAX_HISTORY_LOOKUPS);
    assert_eq!(source.absent_members().len(), MAX_HISTORY_LOOKUPS);
    let receipts = source_receipts(&source);
    assert!(!source.capture_history_member(HISTORY_ADT).unwrap());
    assert_error(
        source.capture_history_member(HISTORY_WDT),
        "source.history_lookup_limit",
    );
    assert_eq!(source_receipts(&source), receipts);
    source.verify_unchanged().unwrap();
}

#[test]
fn history_discovery_requires_prior_absence_proofs_as_well_as_positive_bytes() {
    let temp = Temp::new();
    let data = temp.data();
    fs::write(data.join("dbc.MPQ"), archive(&[entry(MAP, 1)])).unwrap();
    let mut prior = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert!(!prior.capture_history_member(HISTORY_WDT).unwrap());
    let missing = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert_error(
        missing.verify_contains(&prior),
        "source.discovery_absence_changed",
    );
    fs::write(data.join("terrain.MPQ"), archive(&[entry(HISTORY_WDT, 2)])).unwrap();
    let mut now_present = CapturedSource::capture(&data, &[MAP]).unwrap();
    assert!(now_present.capture_history_member(HISTORY_WDT).unwrap());
    assert_error(
        now_present.verify_contains(&prior),
        "source.discovery_absence_changed",
    );
}

const ORACLE_WDT: &str = "World\\Maps\\AuthoredOracle\\AuthoredOracle.wdt";
const ORACLE_ADT: &str = "World\\Maps\\AuthoredOracle\\AuthoredOracle_32_31.adt";
const ORACLE_TILE: crate::Tile = crate::Tile { x: 31, y: 32 };
const ORACLE_PRODUCT: &str = "maps/0013132.map";

fn oracle_entries() -> Vec<Entry> {
    vec![
        raw_entry(
            MAP,
            include_bytes!("../tools/fixtures/Map.dbc.test-fixture").to_vec(),
        ),
        raw_entry(
            AREA,
            include_bytes!("../tools/fixtures/AreaTable.dbc.test-fixture").to_vec(),
        ),
        raw_entry(
            LIQUID,
            include_bytes!("../tools/fixtures/LiquidType.dbc.test-fixture").to_vec(),
        ),
        raw_entry(
            ORACLE_WDT,
            include_bytes!("../tools/fixtures/source.wdt.test-fixture").to_vec(),
        ),
        raw_entry(
            ORACLE_ADT,
            include_bytes!("../tools/fixtures/flat.adt.test-fixture").to_vec(),
        ),
    ]
}

fn install_oracle(data: &Path) -> Vec<u8> {
    let bytes = archive(&oracle_entries());
    fs::write(data.join("terrain.MPQ"), &bytes).unwrap();
    bytes
}

fn change_oracle_source(data: &Path) {
    let mut entries = oracle_entries();
    // A raw Map.dbc field not used for selecting the map still binds the capture.
    entries[0].bytes[28] ^= 1;
    fs::write(data.join("terrain.MPQ"), archive(&entries)).unwrap();
}

#[test]
fn real_mpq_prepare_and_private_publish_match_oracle_without_source_writes() {
    let install = Temp::new();
    let data = install.data();
    let original = install_oracle(&data);
    let destination = Temp::new();
    let output = destination.0.join("selected");
    let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    let epoch = prepared.source_epoch().to_owned();
    assert_eq!(prepared.map_id(), 1);
    let products: Vec<_> = prepared.tiles().collect();
    assert_eq!(products.len(), 1);
    assert_eq!(products[0].0, ORACLE_TILE);
    assert_eq!(products[0].1, include_bytes!("../tools/fixtures/flat.map"));
    prepared.publish(&output).unwrap();
    assert_eq!(
        fs::read(output.join(ORACLE_PRODUCT)).unwrap(),
        include_bytes!("../tools/fixtures/flat.map")
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("terrain.json")).unwrap()).unwrap();
    assert_eq!(manifest["source_epoch"], epoch);
    assert_eq!(manifest["source_resolution_policy"], SOURCE_POLICY);
    assert_eq!(manifest["source_pin"], crate::SOURCE_PIN);
    assert_eq!(manifest["gameplay_admitted"], false);
    assert_eq!(manifest["live_install_lease"], false);
    assert_eq!(manifest["members"].as_array().unwrap().len(), 5);
    assert_eq!(manifest["products"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["products"][0]["file"], ORACLE_PRODUCT);
    assert_eq!(fs::read_dir(&output).unwrap().count(), 2);
    assert_eq!(fs::read_dir(output.join("maps")).unwrap().count(), 1);
    assert_eq!(fs::read(data.join("terrain.MPQ")).unwrap(), original);
    assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(output.join("maps"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        for name in [ORACLE_PRODUCT, "terrain.json"] {
            assert_eq!(
                fs::metadata(output.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn prepared_version_one_extension_change_refuses_before_creating_output() {
    let install = Temp::new();
    let data = install.data();
    let path = data.join("terrain.MPQ");
    let original = archive_version(&oracle_entries(), 1);
    for at in [32, 36, 40, 42] {
        fs::write(&path, &original).unwrap();
        let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
        let destination = Temp::new();
        let output = destination.0.join("selected");
        let mut changed = original.clone();
        changed[at] = 1;
        fs::write(&path, &changed).unwrap();
        assert_error(prepared.publish(&output), "source.ad_header_extension");
        assert!(!output.exists());
        assert_eq!(fs::read(&path).unwrap(), changed);
        assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
    }
}

#[test]
fn prepared_source_mutation_refuses_before_creating_output() {
    let install = Temp::new();
    let data = install.data();
    install_oracle(&data);
    let destination = Temp::new();
    let output = destination.0.join("selected");
    let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    change_oracle_source(&data);
    assert_error(prepared.publish(&output), "source.bytes_changed");
    assert!(!output.exists());
}

#[test]
fn prepare_rejects_invalid_selection_before_accessing_install() {
    let absent = Path::new("authored-nonexistent-install");
    assert_error(crate::prepare(absent, 1, &[]), "selection.bounds");
    assert_error(
        crate::prepare(absent, 1000, &[ORACLE_TILE]),
        "selection.bounds",
    );
    assert_error(
        crate::prepare(absent, 1, &[ORACLE_TILE; 10]),
        "selection.bounds",
    );
    assert_error(
        crate::prepare(absent, 1, &[ORACLE_TILE; 2]),
        "selection.duplicate_tile",
    );
    for tile in [crate::Tile { x: 64, y: 0 }, crate::Tile { x: 0, y: 64 }] {
        assert_error(crate::prepare(absent, 1, &[tile]), "selection.tile_bounds");
    }
}

#[test]
fn prepare_requires_every_selected_member_without_fallback() {
    for missing in [MAP, AREA, LIQUID, ORACLE_WDT, ORACLE_ADT] {
        let install = Temp::new();
        let data = install.data();
        let entries: Vec<_> = oracle_entries()
            .into_iter()
            .filter(|entry| entry.name != missing)
            .collect();
        fs::write(data.join("terrain.MPQ"), archive(&entries)).unwrap();
        assert_error(
            crate::prepare(&data, 1, &[ORACLE_TILE]),
            "source.missing_or_deleted",
        );
    }
}

#[test]
fn publication_rejects_installation_output_and_preserves_existing_output() {
    let install = Temp::new();
    let data = install.data();
    let original = install_oracle(&data);
    let inside = install.0.join("selected");
    let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    assert_error(
        prepared.publish(&inside),
        "output.inside_source_installation",
    );
    assert!(!inside.exists());
    let destination = Temp::new();
    let output = destination.0.join("existing");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("unrelated"), b"preserved").unwrap();
    let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    assert_error(prepared.publish(&output), "output.already_exists");
    assert_eq!(fs::read(output.join("unrelated")).unwrap(), b"preserved");
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    assert_eq!(fs::read(data.join("terrain.MPQ")).unwrap(), original);
}

#[test]
fn publication_faults_never_install_a_complete_marker() {
    for failed_stage in [
        crate::PublishStage::ProductsWritten,
        crate::PublishStage::BeforeManifest,
    ] {
        let install = Temp::new();
        let data = install.data();
        let original = install_oracle(&data);
        let destination = Temp::new();
        let output = destination.0.join("incomplete");
        let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
        let result = prepared.publish_observed(&output, |stage| {
            ensure!(
                stage != failed_stage,
                "authored.injected_publication_failure"
            );
            Ok(())
        });
        assert_error(result, "authored.injected_publication_failure");
        assert!(output.join(ORACLE_PRODUCT).is_file());
        assert!(!output.join("terrain.json").exists());
        assert!(!output.join("terrain.pending").exists());
        assert_eq!(fs::read(data.join("terrain.MPQ")).unwrap(), original);
    }
}

#[test]
fn source_mutation_during_either_publication_stage_prevents_complete_marker() {
    for changed_stage in [
        crate::PublishStage::ProductsWritten,
        crate::PublishStage::BeforeManifest,
    ] {
        let install = Temp::new();
        let data = install.data();
        install_oracle(&data);
        let destination = Temp::new();
        let output = destination.0.join("incomplete");
        let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
        let result = prepared.publish_observed(&output, |stage| {
            if stage == changed_stage {
                change_oracle_source(&data);
            }
            Ok(())
        });
        assert_error(result, "source.bytes_changed");
        assert!(!output.join("terrain.json").exists());
    }
}

#[test]
fn staged_product_mutation_prevents_complete_marker() {
    for changed_stage in [
        crate::PublishStage::ProductsWritten,
        crate::PublishStage::BeforeManifest,
    ] {
        let install = Temp::new();
        let data = install.data();
        install_oracle(&data);
        let destination = Temp::new();
        let output = destination.0.join("incomplete");
        let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
        let result = prepared.publish_observed(&output, |stage| {
            if stage == changed_stage {
                fs::write(output.join(ORACLE_PRODUCT), b"changed after staging")?;
            }
            Ok(())
        });
        assert_error(result, "output.changed_product");
        assert!(!output.join("terrain.json").exists());
    }
}

#[test]
fn oversized_staged_product_is_rejected_without_loading_its_bytes() {
    let install = Temp::new();
    let data = install.data();
    install_oracle(&data);
    let destination = Temp::new();
    let output = destination.0.join("incomplete");
    let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    let result = prepared.publish_observed(&output, |stage| {
        if stage == crate::PublishStage::ProductsWritten {
            fs::OpenOptions::new()
                .write(true)
                .open(output.join(ORACLE_PRODUCT))?
                .set_len(1024 * 1024 * 1024)?;
        }
        Ok(())
    });
    assert_error(result, "output.changed_product");
    assert!(!output.join("terrain.json").exists());
}

#[test]
fn post_commit_error_reports_visible_marker_and_never_reuses_output() {
    let install = Temp::new();
    let data = install.data();
    install_oracle(&data);
    let destination = Temp::new();
    let output = destination.0.join("committed");
    let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    let epoch = prepared.source_epoch().to_owned();
    let result = prepared.publish_observed(&output, |stage| {
        ensure!(
            stage != crate::PublishStage::ManifestLinked,
            "authored.post_commit_failure"
        );
        Ok(())
    });
    assert_error(result, "output.published_but_durability_unconfirmed");
    let marker = fs::read(output.join("terrain.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&marker).unwrap();
    assert_eq!(manifest["source_epoch"], epoch);
    assert_eq!(
        fs::read(output.join(ORACLE_PRODUCT)).unwrap(),
        include_bytes!("../tools/fixtures/flat.map")
    );
    let retry = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    assert_error(retry.publish(&output), "output.already_exists");
    assert_eq!(fs::read(output.join("terrain.json")).unwrap(), marker);
}

#[test]
fn competing_marker_is_never_overwritten() {
    let install = Temp::new();
    let data = install.data();
    install_oracle(&data);
    let destination = Temp::new();
    let output = destination.0.join("incomplete");
    let prepared = crate::prepare(&data, 1, &[ORACLE_TILE]).unwrap();
    let result = prepared.publish_observed(&output, |stage| {
        if stage == crate::PublishStage::BeforeManifest {
            fs::write(output.join("terrain.json"), b"other owner's marker")?;
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(
        fs::read(output.join("terrain.json")).unwrap(),
        b"other owner's marker"
    );
}
