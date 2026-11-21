// SPDX-License-Identifier: GPL-3.0-or-later
//! Unconnected appearance-only adapter. Not an ObjectCreate or native admission path.
use benilla_protocol::ObjectFields;
use benilla_sim::corpse_appearance::Snapshot;

/// Explicit appearance assignments in the 1.12.1 corpse descriptor. Zero words are retained:
/// this is a complete appearance snapshot, not an unknown/missing metadata substitution.
/// Identity, pose, scale, timestamps and registry membership are deliberately absent.
pub fn field_pairs(snapshot: &Snapshot) -> [(u16, u32); 24] {
    let mut pairs = [(0, 0); 24];
    pairs[0] = (12, snapshot.native_display_id);
    for (index, word) in snapshot.equipment.iter().enumerate() {
        pairs[index + 1] = (13 + index as u16, *word);
    }
    pairs[20] = (32, snapshot.bytes_1);
    pairs[21] = (33, snapshot.bytes_2);
    pairs[22] = (34, snapshot.guild_id);
    pairs[23] = (35, snapshot.flags);
    pairs
}

/// Existing renderer-facing field representation, without marking it as a created object.
/// No production caller uses this until the independent corpse lifecycle is composed.
pub fn fields(snapshot: &Snapshot) -> ObjectFields {
    ObjectFields::from_pairs(&field_pairs(snapshot))
}
