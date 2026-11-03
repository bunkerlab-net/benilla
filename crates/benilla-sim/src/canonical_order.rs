// SPDX-License-Identifier: GPL-3.0-or-later
//! Owner-approved native ordering, not observed reference-loader traversal.
// Deviation: CMaNGOS 8ec338a1 ObjectMgr.cpp:3157,3228,3336,7946 and SpellMgr.cpp:1668 use unordered SQL; Player.cpp:15249,15271 uses unordered SQL/SkillStatusMap; ScriptMgr.cpp:118 orders priority only. Native uses a versioned canonical schedule because unspecified reference traversal cannot define portable deterministic preparation and cast occurrences.
use crate::starter_init::{CreationSkill, CreationSpell, CustomStartingItem};
use sha2::{Digest, Sha256};
static IDENTITY: std::sync::LazyLock<[u8; 32]> =
    std::sync::LazyLock::new(|| Sha256::digest(CONTRACT.as_bytes()).into());

pub const CONTRACT: &str = include_str!("canonical_order.json");
pub const POLICY: &str = "benilla-native-canonical-order-v1";

pub fn identity() -> [u8; 32] {
    *IDENTITY
}

pub fn creation_skill(row: &CreationSkill) -> (u32, u32, u32, u32) {
    (row.race_mask, row.class_mask, row.skill, row.step)
}
pub fn creation_spell(row: &CreationSpell) -> (u8, u8, u32) {
    (row.race, row.class, row.spell)
}
pub fn custom_item(row: &CustomStartingItem) -> (u8, u8, u32, u32) {
    (row.race, row.class, row.item, row.amount)
}
pub fn relation(row: &crate::quest_lifecycle::CreatureQuestRelation) -> (u32, u32) {
    (row.creature, row.quest)
}

/// Already normalized skills, never guessed from a prospective SQL skill list.
pub fn entry_skills(
    skills: &std::collections::BTreeMap<u16, crate::starter_init::InitializedSkill>,
) -> Vec<u16> {
    skills.keys().copied().collect()
}

/// Defined script-ID/delay traversal; priority ties use source operands.
pub fn compare_script(
    a: &crate::quest_lifecycle::ScriptRow,
    b: &crate::quest_lifecycle::ScriptRow,
) -> std::cmp::Ordering {
    let key = |r: &crate::quest_lifecycle::ScriptRow| {
        (
            r.id,
            r.delay,
            r.priority,
            r.command,
            r.datalong,
            r.buddy_entry,
            r.search_radius,
            r.data_flags,
            r.dataint,
            r.condition,
        )
    };
    key(a)
        .cmp(&key(b))
        .then_with(|| a.float_bits.cmp(&b.float_bits))
        .then_with(|| a.position.cmp(&b.position))
}
