// SPDX-License-Identifier: GPL-3.0-or-later
//! A normalized, deliberately limited model, not a CMaNGOS table dump.

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
pub const PROFILE: &str = "benilla-world-slice-v1";
pub const CLASSIC_CLIENT_BUILD: u32 = 5875;
pub const REVIEWED_CORE_REVISION: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub const REVIEWED_DATABASE_REVISION: &str = "ec4f596146be6467ea93c57397858e329e2db852";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldExport {
    pub format_version: u32,
    pub profile: String,
    pub source: Source,
    /// Must be empty. A converter must report, never silently omit, required mechanics.
    pub unsupported_features: Vec<String>,
    pub creatures: Vec<CreatureTemplate>,
    pub spawns: Vec<CreatureSpawn>,
    pub quests: Vec<KillQuest>,
    pub quest_relations: Vec<QuestRelation>,
    pub items: Vec<PlainItem>,
    pub abilities: Vec<DirectAbility>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    /// Only original authored content, not an assertion that external data is licensed.
    OriginalFixture { revision: String },
    /// Metadata is user supplied; validation does not establish provenance or legal rights.
    LocalCmangosClassic {
        core_revision: String,
        database_revision: String,
        client_build: u32,
        /// SHA-256 of the local schema description, recorded for audit, not a trust proof.
        schema_sha256: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureTemplate {
    pub entry: u32,
    pub name: String,
    pub display_id: u32,
    pub faction_template_id: u32,
    /// A single resolved level and stat set. Variable levels require another model.
    pub level: u8,
    pub max_health: u32,
    pub armor: u32,
    pub melee_damage: u32,
    pub melee_period_ms: u32,
    pub combat_reach_milliunits: u32,
    pub behavior: CreatureBehavior,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureBehavior {
    Passive,
    /// Fixed damage is a model restriction, not CMaNGOS combat fidelity.
    StationaryMelee,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureSpawn {
    pub spawn_id: u64,
    pub creature_entry: u32,
    pub map_id: u32,
    /// Coordinates in 1/1000 world units, not metres. Quantized by the exporter.
    pub position_milliunits: [i32; 3],
    pub orientation_milliradians: u16,
    pub respawn_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KillQuest {
    pub entry: u32,
    pub title: String,
    pub details: String,
    pub min_level: u8,
    pub max_level: u8,
    /// Ordered objective slots. Kill credit is for these entries only.
    pub objectives: Vec<KillObjective>,
    pub reward_xp: u32,
    pub reward_money: u32,
    pub reward_items: Vec<ItemStack>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KillObjective {
    pub creature_entry: u32,
    pub count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemStack {
    pub item_entry: u32,
    pub count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestRelation {
    pub creature_entry: u32,
    pub quest_entry: u32,
    pub kind: QuestRelationKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestRelationKind {
    Starts,
    Finishes,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlainItem {
    pub entry: u32,
    pub name: String,
    pub display_id: u32,
    pub max_stack: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectAbility {
    pub entry: u32,
    pub name: String,
    pub effect: DirectEffect,
    pub amount: u32,
    pub mana_cost: u32,
    pub cast_time_ms: u32,
    pub cooldown_ms: u32,
    pub global_cooldown_ms: u32,
    pub min_range_milliunits: u32,
    pub max_range_milliunits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectEffect {
    Damage,
    Heal,
}
