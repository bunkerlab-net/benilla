// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 ObjectMgr.cpp:563-573, ScriptMgr.cpp:1162-1200/3485.
// Source GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
use super::*;

/// Raw complete string_id query row. SQL order and names remain source content,
/// even though template validation only asks membership in the first-key ID map.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StringIdRow {
    pub id: i32,
    pub name: String,
}

pub(super) fn retained_name_bytes(input: &Input) -> Result<usize> {
    let rows = match &input.string_ids {
        Rows::Missing => return Ok(0),
        Rows::Complete(rows) => rows,
    };
    rows.iter().try_fold(0usize, |bytes, row| {
        bytes
            .checked_add(row.name.len())
            .filter(|bytes| *bytes <= MAX_CREATURE_INIT_SOURCE_BYTES)
            .ok_or(InitError::Invalid("source retention byte bound"))
    })
}

pub(super) fn prepare(input: &Input) -> Result<[u32; 2]> {
    let rows = input
        .string_ids
        .require("string_id complete cold catalog", input.template.entry)?;
    let pair = [input.template.string_id1, input.template.string_id2];
    // The source uses signed map keys and unsigned template operands. This
    // bounded port refuses negative/high-bit conversion cases instead of guessing.
    if pair.iter().any(|id| *id > i32::MAX as u32) || rows.iter().any(|row| row.id <= 0) {
        return Err(InitError::Unsupported("positive signed string ID domain"));
    }
    let mut by_id = BTreeMap::new();
    for row in rows {
        // ScriptMgr::LoadStringIds emplace keeps the first row for a repeated ID.
        // Its separate name index is not an operand of ExistsStringId.
        by_id.entry(row.id).or_insert(row);
    }
    Ok(pair.map(|id| {
        if id != 0 && by_id.contains_key(&(id as i32)) {
            id
        } else {
            0
        }
    }))
}
