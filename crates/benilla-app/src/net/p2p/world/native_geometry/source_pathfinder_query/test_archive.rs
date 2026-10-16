// SPDX-License-Identifier: GPL-3.0-or-later
// Authored archive encoder follows benilla-native-dbc/tests/extract_chain.rs and
// benilla-mpq/src/crypto.rs (MIT OR Apache-2.0). It exists only in these authored product tests.
fn crypto_table() -> [u32; 0x500] {
    let mut table = [0; 0x500];
    let mut seed: u32 = 0x0010_0001;
    for i in 0..0x100 {
        for j in 0..5 {
            seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2a_aaab;
            let high = (seed & 0xffff) << 16;
            seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2a_aaab;
            table[i + j * 0x100] = high | (seed & 0xffff);
        }
    }
    table
}
fn hash(name: &str, kind: usize) -> u32 {
    let table = crypto_table();
    let (mut a, mut b) = (0x7fed_7fed_u32, 0xeeee_eeee_u32);
    for c in name.bytes().map(|c| {
        if c == b'/' {
            b'\\'
        } else {
            c.to_ascii_uppercase()
        }
    }) {
        a = table[kind + c as usize] ^ a.wrapping_add(b);
        b = u32::from(c)
            .wrapping_add(a)
            .wrapping_add(b)
            .wrapping_add(b << 5)
            .wrapping_add(3);
    }
    a
}
fn encrypt(words: &mut [u32], mut key: u32) {
    let table = crypto_table();
    let mut seed = 0xeeee_eeee_u32;
    for word in words {
        seed = seed.wrapping_add(table[0x400 + (key & 0xff) as usize]);
        let plain = *word;
        *word = plain ^ key.wrapping_add(seed);
        key = (!key << 21).wrapping_add(0x1111_1111) | (key >> 11);
        seed = plain
            .wrapping_add(seed)
            .wrapping_add(seed << 5)
            .wrapping_add(3);
    }
}
pub(super) fn archive(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let slots = 16_u32;
    let hash_pos = 32_u32;
    let block_pos = hash_pos + slots * 16;
    let mut data_pos = block_pos + entries.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; slots as usize * 4];
    let mut blocks = Vec::new();
    for (index, (name, bytes)) in entries.iter().enumerate() {
        let mut slot = (hash(name, 0) & (slots - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (slots as usize - 1);
        }
        hashes[slot * 4..slot * 4 + 4].copy_from_slice(&[
            hash(name, 0x100),
            hash(name, 0x200),
            0,
            index as u32,
        ]);
        blocks.extend([
            data_pos,
            bytes.len() as u32,
            bytes.len() as u32,
            0x8000_0000,
        ]);
        data_pos += bytes.len() as u32;
    }
    encrypt(&mut hashes, hash("(hash table)", 0x300));
    encrypt(&mut blocks, hash("(block table)", 0x300));
    let mut bytes = vec![0_u8; 32];
    for (offset, value) in [
        (0, 0x1a51_504d_u32),
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
    for (_, data) in entries {
        bytes.extend(data);
    }
    bytes
}
