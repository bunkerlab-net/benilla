// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{digest, Error, Position, MAX_ABILITIES, MAX_COORD, MAX_INVENTORY_SLOTS};
use benilla_world_data::{ItemStack, Source, WorldData};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedLoot {
    pub creature: u32,
    pub money: u32,
    pub items: Vec<ItemStack>,
}
/// Immutable, hashed original gameplay policy. This is not a Classic ruleset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub version: u32,
    pub armor_numeric_policy: u32,
    pub tick_ms: u32,
    pub start: Position,
    pub player_health: u32,
    pub player_mana: u32,
    pub melee_damage: u32,
    pub melee_period_ms: u32,
    pub melee_range: u32,
    pub movement_per_tick: u32,
    pub interact_range: u32,
    pub revive_delay_ms: u32,
    pub inventory_slots: usize,
    pub xp_rate_numerator: u32,
    pub xp_rate_denominator: u32,
    /// Total XP for each level, beginning with zero. The fixture table is original.
    pub level_thresholds: Vec<u64>,
    pub starting_abilities: Vec<u32>,
    /// Guaranteed, fixed drops; no PRNG or probabilistic loot exists in v1.
    pub loot: Vec<FixedLoot>,
}
impl Rules {
    pub fn original_camp() -> Self {
        Self {
            version: 2,
            armor_numeric_policy: benilla_sim::classic::NUMERIC_POLICY_VERSION,
            tick_ms: 100,
            start: Position {
                map: 0,
                xyz: [0, 0, 0],
            },
            player_health: 60,
            player_mana: 30,
            melee_damage: 10,
            melee_period_ms: 1000,
            melee_range: 2000,
            movement_per_tick: 1000,
            interact_range: 4000,
            revive_delay_ms: 3000,
            inventory_slots: 16,
            xp_rate_numerator: 1,
            xp_rate_denominator: 1,
            level_thresholds: vec![0, 200, 500, 900],
            starting_abilities: vec![930001, 930002],
            loot: vec![FixedLoot {
                creature: 900002,
                money: 1,
                items: vec![ItemStack {
                    item_entry: 920001,
                    count: 1,
                }],
            }],
        }
    }
    pub(crate) fn validate(&self, data: &WorldData) -> Result<(), Error> {
        // Import support must be explicitly implemented, never inferred from a matching schema.
        if !matches!(data.export().source, Source::OriginalFixture { .. })
            || data
                .export()
                .creatures
                .iter()
                .any(|c| c.faction_template_id != 0)
        {
            return Err(Error::UnsupportedContent);
        }
        if self.version != 2
            || self.armor_numeric_policy != benilla_sim::classic::NUMERIC_POLICY_VERSION
            || !(1..=1000).contains(&self.tick_ms)
            || self.start.map > 65535
            || self
                .start
                .xyz
                .iter()
                .any(|x| x.unsigned_abs() > MAX_COORD as u32)
            || !(1..=1_000_000).contains(&self.player_health)
            || self.player_mana > 1_000_000
            || !(1..=1_000_000).contains(&self.melee_damage)
            || !(1..=86_400_000).contains(&self.melee_period_ms)
            || !(1..=100_000).contains(&self.melee_range)
            || !(1..=100_000).contains(&self.movement_per_tick)
            || !(1..=100_000).contains(&self.interact_range)
            || !(1..=86_400_000).contains(&self.revive_delay_ms)
            || !(1..=MAX_INVENTORY_SLOTS).contains(&self.inventory_slots)
            || !(1..=1_000_000).contains(&self.xp_rate_numerator)
            || !(1..=1_000_000).contains(&self.xp_rate_denominator)
            || u64::from(self.xp_rate_numerator) > u64::from(self.xp_rate_denominator) * 1000
            || u64::from(self.xp_rate_numerator) * 1000 < u64::from(self.xp_rate_denominator)
            || self.level_thresholds.is_empty()
            || self.level_thresholds.len() > 60
            || self.level_thresholds[0] != 0
            || self.level_thresholds.windows(2).any(|w| w[0] >= w[1])
            || self
                .level_thresholds
                .iter()
                .any(|x| *x > crate::MAX_PROGRESSION)
            || self.starting_abilities.len() > MAX_ABILITIES
            || !strictly_sorted(&self.starting_abilities)
            || self
                .starting_abilities
                .iter()
                .any(|id| data.ability(*id).is_none())
            || self.loot.len() > crate::MAX_NPCS
        {
            return Err(Error::InvalidRules);
        }
        let mut seen = BTreeSet::new();
        for loot in &self.loot {
            if data.creature(loot.creature).is_none()
                || !seen.insert(loot.creature)
                || loot.money > 1_000_000
                || loot.items.len() > 4
                || !strictly_sorted(&loot.items.iter().map(|x| x.item_entry).collect::<Vec<_>>())
                || loot
                    .items
                    .iter()
                    .any(|x| data.item(x.item_entry).is_none() || x.count == 0 || x.count > 10_000)
            {
                return Err(Error::InvalidRules);
            }
        }
        if !strictly_sorted(&self.loot.iter().map(|x| x.creature).collect::<Vec<_>>()) {
            return Err(Error::InvalidRules);
        }
        Ok(())
    }
    pub fn digest(&self) -> crate::Digest {
        digest(b"benilla-world-rules-v1\0", self)
    }
    pub(crate) fn ticks(&self, ms: u32) -> u64 {
        u64::from(ms).div_ceil(u64::from(self.tick_ms))
    }
    pub(crate) fn level(&self, xp: u64) -> u8 {
        self.level_thresholds.partition_point(|x| *x <= xp) as u8
    }
    pub(crate) fn rated_xp(&self, xp: u64) -> u64 {
        xp * u64::from(self.xp_rate_numerator) / u64::from(self.xp_rate_denominator)
    }
}
pub(crate) fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|w| w[0] < w[1])
}
