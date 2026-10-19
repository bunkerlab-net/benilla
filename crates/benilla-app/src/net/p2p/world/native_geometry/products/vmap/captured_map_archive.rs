// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only valid version-0 MPQ encoder localized from native-vmap-source/source_tests.rs.
//! Table primitives originate in benilla-mpq/src/crypto.rs (MIT OR Apache-2.0).
//! Inputs are newly authored fixtures; this never reads or modifies a client install.

pub(super) struct Entry {
    pub(super) name: String,
    pub(super) bytes: Vec<u8>,
    pub(super) flags: u32,
    pub(super) declared: u32,
}

pub(super) fn raw_entry(name: &str, bytes: Vec<u8>) -> Entry {
    Entry {
        name: name.to_owned(),
        declared: bytes.len() as u32,
        bytes,
        flags: 0x8000_0000,
    }
}

pub(super) fn archive(entries: &[Entry]) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    let slots = 64u32;
    assert!(entries.len() < slots as usize);
    let header_bytes = 32u32;
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
    bytes[12..14].copy_from_slice(&0u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&3u16.to_le_bytes());
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend(word.to_le_bytes());
    }
    for entry in entries {
        bytes.extend(&entry.bytes);
    }
    bytes
}

mod crypto {
    const fn encryption_table() -> [u32; 0x500] {
        let mut table = [0u32; 0x500];
        let mut seed: u32 = 0x0010_0001;
        let mut i1 = 0;
        while i1 < 0x100 {
            let mut i2 = 0;
            while i2 < 5 {
                let idx = i1 + i2 * 0x100;
                seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2A_AAAB;
                let t1 = (seed & 0xFFFF) << 0x10;
                seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2A_AAAB;
                let t2 = seed & 0xFFFF;
                table[idx] = t1 | t2;
                i2 += 1;
            }
            i1 += 1;
        }
        table
    }

    static TABLE: [u32; 0x500] = encryption_table();

    /// Hash-type selectors: the table-quadrant offsets the client uses.
    pub mod hash_type {
        /// Hash-table slot index.
        pub const TABLE_OFFSET: u32 = 0x000;
        /// Filename verifier A, stored in the hash entry.
        pub const NAME_A: u32 = 0x100;
        /// Filename verifier B, stored in the hash entry.
        pub const NAME_B: u32 = 0x200;
        /// File and table decryption key.
        pub const FILE_KEY: u32 = 0x300;
    }

    /// Blizzard's string hash: `/` reads as `\`, and only `a`-`z` are uppercased, as the client's
    /// `AsciiToUpper` table does.
    pub fn hash_string(name: &str, kind: u32) -> u32 {
        let mut seed1: u32 = 0x7FED_7FED;
        let mut seed2: u32 = 0xEEEE_EEEE;
        for &b in name.as_bytes() {
            let ch = match b {
                b'/' => b'\\',
                other => other.to_ascii_uppercase(),
            } as u32;
            seed1 = TABLE[(kind.wrapping_add(ch)) as usize] ^ seed1.wrapping_add(seed2);
            seed2 = ch
                .wrapping_add(seed1)
                .wrapping_add(seed2)
                .wrapping_add(seed2 << 5)
                .wrapping_add(3);
        }
        seed1
    }

    pub(super) fn encrypt_block(data: &mut [u32], mut key: u32) {
        let mut seed: u32 = 0xEEEE_EEEE;
        for v in data.iter_mut() {
            seed = seed.wrapping_add(TABLE[0x400 + (key & 0xFF) as usize]);
            let plain = *v;
            *v = plain ^ key.wrapping_add(seed);
            key = (!key << 0x15).wrapping_add(0x1111_1111) | (key >> 0x0B);
            seed = plain
                .wrapping_add(seed)
                .wrapping_add(seed << 5)
                .wrapping_add(3);
        }
    }
}
