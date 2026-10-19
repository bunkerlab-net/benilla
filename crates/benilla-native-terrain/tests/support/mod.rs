// SPDX-License-Identifier: GPL-3.0-or-later
//! Original authored transport fixtures; never client assets.
#[path = "../../../benilla-mpq/src/crypto.rs"]
pub(crate) mod crypto;
pub struct Entry {
    name: String,
    bytes: Vec<u8>,
    flags: u32,
    declared: u32,
}

pub fn raw_entry(name: &str, bytes: Vec<u8>) -> Entry {
    Entry {
        name: name.to_owned(),
        declared: bytes.len() as u32,
        bytes,
        flags: 0x8000_0000,
    }
}

pub fn archive(entries: &[Entry]) -> Vec<u8> {
    archive_with_header(entries, 0, 32, 0)
}

pub fn version_one_archive(entries: &[Entry]) -> Vec<u8> {
    archive_with_header(entries, 1, 44, 3)
}

fn archive_with_header(
    entries: &[Entry],
    version: u16,
    header_bytes: u32,
    sector_shift: u16,
) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    let slots = 64u32;
    // Index and encrypted block-table member positions follow the physical header.
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
    bytes[14..16].copy_from_slice(&sector_shift.to_le_bytes());
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend(word.to_le_bytes());
    }
    for entry in entries {
        bytes.extend(&entry.bytes);
    }
    bytes
}
