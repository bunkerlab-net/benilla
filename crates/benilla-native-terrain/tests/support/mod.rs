// SPDX-License-Identifier: GPL-3.0-or-later
//! Original authored transport fixtures; never client assets.
#[path = "../../../benilla-mpq/src/crypto.rs"]
mod crypto;
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
        (0, 0x1a51_504d_u32),
        (4, 32),
        (8, data_pos),
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
