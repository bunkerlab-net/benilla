//! Authored MPQs exercise the production inventory read boundary without installation data.
use super::tests::{chunk, wdt};
use super::*;
use crate::authored_mpq_crypto::{encrypt_block, hash_string, hash_type};
use crate::models::native_wmo::tests::{group, root};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const WDT: &str = "World\\Maps\\Test\\Test.wdt";
const ADT: &str = "World\\Maps\\Test\\Test_32_32.adt";

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "benilla-inventory-receipts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, members: &[(&str, Vec<u8>)]) {
        std::fs::write(self.0.join(name), archive(members)).unwrap();
    }
    fn load(&self) -> Result<NativeM2MapInventory> {
        NativeM2MapInventory::load(&mut Chain::open(&self.0)?, "Test")
    }
    fn bounded(&self, limits: SourceLimits) -> Result<NativeM2MapInventory> {
        NativeM2MapInventory::load_with_limits(&mut Chain::open(&self.0)?, "Test", limits)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let slots = (members.len() as u32 + 1).next_power_of_two();
    let hash_pos = 32;
    let block_pos = hash_pos + slots * 16;
    let mut data_pos = block_pos + members.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; slots as usize * 4];
    let mut blocks = Vec::new();
    for (index, (name, data)) in members.iter().enumerate() {
        let mut slot = (hash_string(name, hash_type::TABLE_OFFSET) & (slots - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (slots as usize - 1);
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
        (24, slots),
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

fn terrain_members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (WDT, wdt()),
        (ADT, super::super::native_coverage_tests::source()),
    ]
}

fn check_receipts(source: &NativeInventorySource, members: &[(&str, Vec<u8>)], winner: &str) {
    assert_eq!(source.receipts().len(), members.len());
    for (name, bytes) in members {
        let receipt = source.receipts().find(|r| r.member() == *name).unwrap();
        assert_eq!(receipt.winning_archive(), winner);
        assert_eq!(receipt.bytes(), bytes.len() as u64);
        assert_eq!(receipt.sha256(), <[u8; 32]>::from(Sha256::digest(bytes)));
    }
}

#[test]
fn genuine_load_retains_exact_names_whole_bytes_and_unforgeable_owner() {
    let temp = Temp::new();
    let members = terrain_members();
    temp.write("TeRrAiN.MPQ", &members);
    let first = temp.load().unwrap();
    let source = first.source_provenance().unwrap();
    assert_eq!(source.policy(), "benilla-client-chain-native-inventory-v1");
    assert_eq!(source.directory(), "Test");
    assert_eq!(source.inventory_digest(), first.source_digest());
    check_receipts(source, &members, "TeRrAiN.MPQ");

    let clone = source.clone();
    assert!(source.same_owner(&clone));
    let second = temp.load().unwrap();
    let second = second.source_provenance().unwrap();
    assert!(!source.same_owner(second));
    assert_eq!(source.receipt_digest(), second.receipt_digest());
    assert_eq!(source.inventory_digest(), second.inventory_digest());

    let callback = NativeM2MapInventory::read("Test", |name| {
        Ok(members.iter().find(|(n, _)| *n == name).unwrap().1.clone())
    })
    .unwrap();
    assert_eq!(callback.source_digest(), source.inventory_digest());
    assert!(callback.source_provenance().is_none());
    drop(first);
    assert_eq!(clone.directory(), "Test");
    check_receipts(&clone, &members, "TeRrAiN.MPQ");
}

#[test]
fn equal_raw_bytes_under_another_winner_change_only_the_receipt_identity() {
    let temp = Temp::new();
    let members = terrain_members();
    temp.write("terrain.MPQ", &members);
    let base = temp.load().unwrap();
    temp.write("PaTcH.MPQ", &members);
    let patched = temp.load().unwrap();
    assert_eq!(base.source_digest(), patched.source_digest());
    let base = base.source_provenance().unwrap();
    let patched = patched.source_provenance().unwrap();
    assert_ne!(base.receipt_digest(), patched.receipt_digest());
    check_receipts(base, &members, "terrain.MPQ");
    check_receipts(patched, &members, "PaTcH.MPQ");
}

#[test]
fn hull_less_adt_m2_is_still_a_consumed_member() {
    let temp = Temp::new();
    let mut members = vec![
        (WDT, wdt()),
        (ADT, model_tests::placed()),
        ("world\\tree.m2", model_tests::empty_model()),
    ];
    temp.write("terrain.MPQ", &members);
    let before = temp.load().unwrap();
    assert!(before
        .placements()
        .next()
        .unwrap()
        .1
        .hull()
        .positions
        .is_empty());
    check_receipts(before.source_provenance().unwrap(), &members, "terrain.MPQ");
    members[2].1[0x140] = 1;
    temp.write("terrain.MPQ", &members);
    let after = temp.load().unwrap();
    check_receipts(after.source_provenance().unwrap(), &members, "terrain.MPQ");
    assert_ne!(
        before.source_provenance().unwrap().receipt_digest(),
        after.source_provenance().unwrap().receipt_digest()
    );
}

fn payload(bytes: &[u8], magic: &[u8; 4]) -> Vec<u8> {
    let mut at = 0;
    loop {
        let n = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if &bytes[at..at + 4] == magic {
            return bytes[at + 8..at + 8 + n].to_vec();
        }
        at += 8 + n;
    }
}

fn wmo_members() -> Vec<(&'static str, Vec<u8>)> {
    // Two groups, one hull-less, and a hull-less MODD M2 in an inactive set.
    let base = root(true);
    let mut header = payload(&base, b"DHOM");
    for at in [4, 20, 24] {
        header[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
    }
    let mut info = payload(&base, b"IGOM");
    info.extend(info.clone());
    let names = b"World\\Chair.mdx\0World\\Unused.mdx\0";
    let mut doodads = payload(&base, b"DDOM");
    let mut inactive = doodads.clone();
    inactive[..4].copy_from_slice(&(b"World\\Chair.mdx\0".len() as u32).to_le_bytes());
    doodads.extend(inactive);
    let mut sets = payload(&base, b"SDOM");
    let mut inactive = sets.clone();
    inactive[20..24].copy_from_slice(&1u32.to_le_bytes());
    sets.extend(inactive);
    let mut root = chunk(b"REVM", &17u32.to_le_bytes());
    for (magic, data) in [
        (b"DHOM", header.as_slice()),
        (b"IGOM", info.as_slice()),
        (b"NDOM", names.as_slice()),
        (b"DDOM", doodads.as_slice()),
        (b"SDOM", sets.as_slice()),
    ] {
        root.extend(chunk(magic, data));
    }
    let mut empty = payload(&group(false), b"PGOM")[..68].to_vec();
    for magic in [b"TVOM", b"IVOM", b"YPOM"] {
        empty.extend(chunk(magic, &[]));
    }
    empty.extend(chunk(b"RDOM", &1u16.to_le_bytes()));
    let mut empty_group = chunk(b"REVM", &17u32.to_le_bytes());
    empty_group.extend(chunk(b"PGOM", &empty));
    vec![
        (WDT, wdt()),
        (ADT, wmo_tests::tile()),
        ("world\\house.wmo", root),
        ("world\\house_000.wmo", group(true)),
        ("world\\house_001.wmo", empty_group),
        ("world\\chair.m2", model_tests::empty_model()),
        ("world\\unused.m2", model_tests::empty_model()),
    ]
}

#[test]
fn wmo_all_groups_and_inactive_hull_less_doodads_have_full_raw_receipts() {
    let temp = Temp::new();
    let members = wmo_members();
    temp.write("terrain.MPQ", &members);
    let before = temp.load().unwrap();
    let (_, model) = before.wmo(84).unwrap();
    assert_eq!(model.collision().root().group_count(), 2);
    assert_eq!(model.collision().root().selected_doodads(0), Some(vec![0]));
    assert!(model.doodad(1).unwrap().hull().positions.is_empty());
    check_receipts(before.source_provenance().unwrap(), &members, "terrain.MPQ");

    for changed_index in 0..members.len() {
        let mut changed = members.clone();
        let (name, bytes) = &mut changed[changed_index];
        if name.ends_with(".wdt") {
            // A reserved MPHD byte, outside the terrain/grid semantics.
            bytes[12 + 8 + 4] = 1;
        } else if name.ends_with(".m2") {
            bytes[0x140] = 1;
        } else if name.ends_with(".adt") {
            bytes.extend(chunk(b"XETM", b"authored-unused-texture\0"));
        } else {
            bytes.extend(chunk(b"TEST", &[1]));
        }
        temp.write("terrain.MPQ", &changed);
        let after = temp.load().unwrap();
        check_receipts(after.source_provenance().unwrap(), &changed, "terrain.MPQ");
        assert_ne!(
            before.source_digest(),
            after.source_digest(),
            "{}",
            members[changed_index].0
        );
        assert_ne!(
            before.source_provenance().unwrap().receipt_digest(),
            after.source_provenance().unwrap().receipt_digest(),
            "{}",
            members[changed_index].0
        );
    }
    for omitted_index in 0..members.len() {
        let mut incomplete = members.clone();
        incomplete.remove(omitted_index);
        temp.write("terrain.MPQ", &incomplete);
        assert!(temp.load().is_err(), "{}", members[omitted_index].0);
    }
}

#[test]
fn genuine_load_enforces_count_name_and_member_bounds_with_reduced_limits() {
    let temp = Temp::new();
    let members = terrain_members();
    temp.write("terrain.MPQ", &members);
    let names = members
        .iter()
        .map(|(name, _)| name.len() * 2 + "terrain.MPQ".len())
        .sum();
    let exact = SourceLimits {
        receipts: members.len(),
        name_bytes: names,
        wdt_bytes: members[0].1.len(),
        member_bytes: members[1].1.len(),
    };
    assert!(temp.bounded(exact).is_ok());
    for reduced in [
        SourceLimits {
            receipts: exact.receipts - 1,
            ..exact
        },
        SourceLimits {
            name_bytes: exact.name_bytes - 1,
            ..exact
        },
        SourceLimits {
            wdt_bytes: exact.wdt_bytes - 1,
            ..exact
        },
        SourceLimits {
            member_bytes: exact.member_bytes - 1,
            ..exact
        },
    ] {
        assert!(temp.bounded(reduced).is_err());
    }
}
