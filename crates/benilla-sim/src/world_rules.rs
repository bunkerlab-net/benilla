// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// World.cpp:411-427,443,456,464-476,548-553,653,661-662,686,703-704,743;
// GPL-2.0-or-later, translation GPL-3.0-or-later.
//! Explicit bounded world configuration, not a capture of a realm's mangosd.conf.
use crate::{classic_spell::SpellFloat, loot, player_progression as pg, quest, reputation};
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 2;
pub const STOCK_PRESET: &str = "cmangos-8ec338a1-stock-world-v2";

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
    pub combat: CombatRules,
}

/// World::LoadConfigSettings combat keys. Distances and rates use bounded binary32 bits.
/// CreatureFamilyFleeDelay is the upstream key, not CreatureFamilyFleeAssistanceDelay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    pub aggro_rate_bits: u32,
    pub family_assistance_radius_bits: u32,
    pub family_assistance_delay_ms: u32,
    pub family_flee_assistance_radius_bits: u32,
    pub family_flee_delay_ms: u32,
    pub check_for_help_radius_bits: u32,
    pub check_for_help_aggro_delay_ms: u32,
    pub linking_aggro_delay_ms: u32,
    pub respawn_aggro_delay_ms: u32,
    pub relocation_notify_delay_ms: u32,
    pub leash_radius_bits: u32,
    pub listen_say_range_bits: u32,
    pub listen_yell_range_bits: u32,
    pub listen_text_emote_range_bits: u32,
}
impl CombatRules {
    pub fn cmangos_classic_stock() -> Self {
        Self {
            aggro_rate_bits: 0x3f80_0000,
            family_assistance_radius_bits: 0x40a0_0000,
            family_assistance_delay_ms: 1500,
            family_flee_assistance_radius_bits: 0x41f0_0000,
            family_flee_delay_ms: 10000,
            check_for_help_radius_bits: 0x40a0_0000,
            check_for_help_aggro_delay_ms: 2000,
            linking_aggro_delay_ms: 2000,
            respawn_aggro_delay_ms: 5000,
            relocation_notify_delay_ms: 1000,
            leash_radius_bits: 0x41f0_0000,
            listen_say_range_bits: 0x41c8_0000,
            listen_yell_range_bits: 0x4396_0000,
            listen_text_emote_range_bits: 0x41c8_0000,
        }
    }
    pub fn validate(&self) -> pg::Result<()> {
        for bits in [
            self.aggro_rate_bits,
            self.family_assistance_radius_bits,
            self.family_flee_assistance_radius_bits,
            self.check_for_help_radius_bits,
            self.leash_radius_bits,
            self.listen_say_range_bits,
            self.listen_yell_range_bits,
            self.listen_text_emote_range_bits,
        ] {
            quest::Binary32Rate::from_bits(bits).map_err(|_| {
                pg::Error::Invalid("world combat scalar outside bounded binary32 domain")
            })?;
        }
        Ok(())
    }
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
            combat: CombatRules::cmangos_classic_stock(),
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
        self.combat.validate()?;
        Ok(())
    }
    pub fn quest_xp_rate(&self) -> Result<quest::Binary32Rate, quest::QuestError> {
        quest::Binary32Rate::from_bits(self.quest_xp_rate_bits)
    }
    pub fn money_rate(&self) -> Result<quest::Binary32Rate, quest::QuestError> {
        quest::Binary32Rate::from_bits(self.loot.money_rate_bits)
    }
}
