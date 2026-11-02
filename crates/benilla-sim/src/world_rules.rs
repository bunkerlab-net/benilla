// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// World.cpp:411-427,456,548-553,653,686; GPL-2.0-or-later, translation GPL-3.0-or-later.
//! Explicit bounded world configuration, not a capture of a realm's mangosd.conf.
use crate::{classic_spell::SpellFloat, loot, player_progression as pg, quest, reputation};
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const STOCK_PRESET: &str = "cmangos-8ec338a1-stock-progression-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWorldRules {
    pub version: u32,
    pub start_player_level: u32,
    pub start_player_money: u32,
    pub progression: pg::Rules,
    pub kill: pg::kill_xp::Rules,
    pub quest_xp_rate_bits: u32,
    pub reputation: reputation::Config,
    pub loot: loot::Rules,
}
impl NativeWorldRules {
    /// Named World::LoadConfigSettings defaults, never an implicit missing-input fallback.
    pub fn cmangos_classic_stock() -> Self {
        Self {
            version: VERSION,
            start_player_level: 1,
            start_player_money: 0,
            progression: pg::Rules {
                max_player_level: 60,
                always_max_skill_for_level: false,
                global_skill_max: 300,
                talent_rate_bits: 0x3f80_0000,
            },
            kill: pg::kill_xp::Rules {
                kill_rate_bits: 0x3f80_0000,
            },
            quest_xp_rate_bits: 0x3f80_0000,
            reputation: reputation::Config {
                gain_rate: SpellFloat::ONE,
                low_level_kill_rate: SpellFloat::from_bits(0x3e4c_cccd).expect("0.2f"),
                low_level_quest_rate: SpellFloat::from_bits(0x3e4c_cccd).expect("0.2f"),
            },
            loot: loot::Rules {
                quality_rate_bits: [0x3f80_0000; 7],
                quest_drop_rate: loot::QuestDropRate {
                    multiplier_bits: 0x3f80_0000,
                },
                referenced_rate_bits: 0x3f80_0000,
                money_rate_bits: 0x3f80_0000,
                corpse_empty_loot_show: true,
            },
        }
    }
    /// Reject unsupported values instead of imitating the loader's warning-and-reset repairs.
    pub fn validate(&self) -> pg::Result<()> {
        if self.version != VERSION {
            return Err(pg::Error::Unsupported("native world rules version"));
        }
        if !(1..=pg::MAX_SUPPORTED_LEVEL).contains(&self.progression.max_player_level) {
            return Err(pg::Error::Unsupported("MaxPlayerLevel outside 1..=60"));
        }
        if self.progression
            != pg::Rules::from_classic_config(
                self.progression.max_player_level,
                self.progression.always_max_skill_for_level,
                self.progression.talent_rate_bits,
            )?
        {
            return Err(pg::Error::Invalid("world progression rules"));
        }
        if self.start_player_level != 1 {
            return Err(pg::Error::Unsupported("StartPlayerLevel other than one"));
        }
        if self.start_player_money > crate::quest_lifecycle::MAX_MONEY {
            return Err(pg::Error::Invalid("StartPlayerMoney"));
        }
        for bits in [
            self.progression.talent_rate_bits,
            self.kill.kill_rate_bits,
            self.quest_xp_rate_bits,
            self.reputation.gain_rate.bits(),
            self.reputation.low_level_kill_rate.bits(),
            self.reputation.low_level_quest_rate.bits(),
            self.loot.quest_drop_rate.multiplier_bits,
            self.loot.referenced_rate_bits,
            self.loot.money_rate_bits,
        ]
        .into_iter()
        .chain(self.loot.quality_rate_bits)
        {
            quest::Binary32Rate::from_bits(bits)
                .map_err(|_| pg::Error::Invalid("world rate outside bounded binary32 domain"))?;
        }
        Ok(())
    }
    pub fn quest_xp_rate(&self) -> Result<quest::Binary32Rate, quest::QuestError> {
        quest::Binary32Rate::from_bits(self.quest_xp_rate_bits)
    }
    pub fn money_rate(&self) -> Result<quest::Binary32Rate, quest::QuestError> {
        quest::Binary32Rate::from_bits(self.loot.money_rate_bits)
    }
}
