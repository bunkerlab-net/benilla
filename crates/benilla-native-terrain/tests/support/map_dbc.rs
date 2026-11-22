// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored 42-field Map.dbc rows; no installation data.

pub fn map_dbc(rows: &[(u32, &[u8])]) -> Vec<u8> {
    let mut records = Vec::new();
    let mut strings = vec![0];
    for (id, name) in rows {
        let mut row = [0u32; 42];
        row[0] = *id;
        row[1] = strings.len() as u32;
        for field in row {
            records.extend(field.to_le_bytes());
        }
        strings.extend_from_slice(name);
        strings.push(0);
    }
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len(), 42, 42 * 4, strings.len()] {
        bytes.extend((value as u32).to_le_bytes());
    }
    bytes.extend(records);
    bytes.extend(strings);
    bytes
}
