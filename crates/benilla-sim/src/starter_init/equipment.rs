// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Player::_ApplyItemBonuses and DurabilityPointsLoss callback ordering.
use super::{
    stats::{ModifierSnapshot, Stats},
    Attributes, EquippedItem, InitError, ItemRow, ItemStat, Resources, Result,
};
use crate::{
    classic_spell::SpellFloat,
    mage_resource::{ManaRegen, ManaRegenModifiers, SpellCritModifiers},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentItemModifiers {
    pub slot: u8,
    pub item_id: u32,
    pub max_durability: u32,
    pub weapon: bool,
    pub stats: [ItemStat; 10],
    pub armor: u32,
    pub resistances: [u32; 6],
}
/// Wire-friendly fields; callers may serialize these, but restore also requires
/// the fresh authoritative recipe bound by their profile/content digest.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatRecipeSnapshot {
    pub version: u32,
    /// Actual create-row level; restore requires an authoritative recipe at this level.
    pub level: u32,
    pub modifiers: ModifierSnapshot,
    pub items: Vec<EquipmentItemModifiers>,
    pub broken_slots: Vec<u8>,
    /// Original objects currently absent from their original equipment slots.
    pub inactive_slots: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatRecipe {
    snapshot: StatRecipeSnapshot,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentRecompute {
    pub effective_attributes: Attributes,
    pub resistances: [i32; 7],
    pub resources: Resources,
    pub mana_regeneration: ManaRegen,
    pub spell_crit_percent: SpellFloat,
    pub disabled_weapon_slots: BTreeSet<u8>,
    /// Resource state after each nonzero item stat, armor, or resistance modifier.
    pub modifier_steps: Vec<Resources>,
    /// Nonzero item attribute callbacks, in ItemStat order. Each tuple contains
    /// the source Stats index and effective attributes immediately after it.
    pub attribute_steps: Vec<(u8, Attributes)>,
}
fn rates(level: u32, stats: &Stats) -> Result<(ManaRegen, SpellFloat)> {
    let regen = crate::mage_resource::update_mana_regen(
        stats.effective.0[4] as u32,
        ManaRegenModifiers {
            spirit_multiplier: SpellFloat::from_bits(0x3f800000)
                .map_err(|_| InitError::Numeric("one"))?,
            flat_mana_per_five: 0,
            interrupted_percent: 0,
        },
    )
    .map_err(|_| InitError::Numeric("equipment mage regen"))?;
    let crit = crate::mage_resource::mage_spell_crit(
        level,
        stats.effective.0[3] as u32,
        SpellCritModifiers {
            all_schools: 0,
            school: 0,
        },
    )
    .map_err(|_| InitError::Numeric("equipment mage crit"))?;
    Ok((regen, crit))
}
impl StatRecipe {
    pub(super) fn new(
        stats: &Stats,
        equipped: &BTreeMap<u8, EquippedItem>,
        items: &[ItemRow],
    ) -> Result<Self> {
        let mut modifiers = vec![];
        for (&slot, equipped) in equipped {
            let item = items
                .iter()
                .find(|r| r.id == equipped.item)
                .ok_or(InitError::Invalid("equipment recipe item missing"))?;
            if item.max_durability > i32::MAX as u32 {
                return Err(InitError::Numeric("initial max durability"));
            }
            modifiers.push(EquipmentItemModifiers {
                slot,
                item_id: item.id,
                max_durability: item.max_durability,
                weapon: item.class == 2,
                stats: item.stats,
                armor: item.armor,
                resistances: item.resistances,
            });
        }
        Ok(Self {
            snapshot: StatRecipeSnapshot {
                version: 4,
                level: 1,
                modifiers: stats.snapshot(),
                items: modifiers,
                broken_slots: vec![],
                inactive_slots: vec![],
            },
        })
    }
    pub fn snapshot(&self) -> StatRecipeSnapshot {
        self.snapshot.clone()
    }
    /// Replace create values from the caller's validated progression catalog.
    /// This does not authorize those rows: callers must bind them to the profile
    /// before use. Static modifiers and broken slots remain unchanged. Current
    /// health/mana are never stored in this recipe and must be supplied separately.
    pub fn rebase(
        &self,
        level: u32,
        create: Attributes,
        create_health: u32,
        create_mana: u32,
    ) -> Result<Self> {
        if !(1..=crate::player_progression::MAX_SUPPORTED_LEVEL).contains(&level) {
            return Err(InitError::Invalid("equipment recipe level"));
        }
        let mut next = self.clone();
        next.snapshot.level = level;
        next.snapshot.modifiers.create = create;
        next.snapshot.modifiers.create_health = create_health;
        next.snapshot.modifiers.create_mana = create_mana;
        let stats = Stats::restore(&next.snapshot.modifiers)?;
        rates(level, &stats)?;
        Ok(next)
    }
    /// Restore only if all immutable and mutable cells agree with replaying the
    /// supplied broken flags against this profile's authoritative fresh recipe.
    /// This never supplies authority for new content, item types or modifiers.
    pub fn restore(fresh: &Self, snapshot: &StatRecipeSnapshot) -> Result<Self> {
        if !fresh.snapshot.inactive_slots.is_empty()
            || !snapshot.inactive_slots.is_empty()
            || !fresh.snapshot.broken_slots.is_empty()
            || snapshot.version != 4
            || fresh.snapshot.version != 4
            || snapshot.level != fresh.snapshot.level
            || !(1..=crate::player_progression::MAX_SUPPORTED_LEVEL).contains(&snapshot.level)
            || snapshot.items != fresh.snapshot.items
            || snapshot.broken_slots.windows(2).any(|x| x[0] >= x[1])
        {
            return Err(InitError::Invalid(
                "equipment recipe snapshot shape/identity",
            ));
        }
        let mut expected = fresh.clone();
        for &slot in &snapshot.broken_slots {
            let stats = Stats::restore(&expected.snapshot.modifiers)?;
            expected.set_item_broken(slot, true, stats.resources)?;
        }
        if expected.snapshot != *snapshot {
            return Err(InitError::Invalid("equipment recipe modifier mismatch"));
        }
        Ok(expected)
    }
    /// Validate a live resource snapshot and derive the current equipment fields.
    pub fn current(&self, live: Resources) -> Result<EquipmentRecompute> {
        let mut stats = Stats::restore(&self.snapshot.modifiers)?;
        self.validate_resources(&stats, live)?;
        stats.resources = live;
        self.result(stats, vec![], vec![])
    }
    fn validate_resources(&self, stats: &Stats, live: Resources) -> Result<()> {
        if live.max_health != stats.resources.max_health
            || live.max_mana != stats.resources.max_mana
            || live.health > live.max_health
            || live.mana > live.max_mana
        {
            return Err(InitError::Invalid("equipment live resources/maxima"));
        }
        Ok(())
    }
    fn result(
        &self,
        stats: Stats,
        modifier_steps: Vec<Resources>,
        attribute_steps: Vec<(u8, Attributes)>,
    ) -> Result<EquipmentRecompute> {
        let (mana_regeneration, spell_crit_percent) = rates(self.snapshot.level, &stats)?;
        let disabled_weapon_slots = self
            .snapshot
            .items
            .iter()
            .filter(|i| i.weapon && self.snapshot.broken_slots.contains(&i.slot))
            .map(|i| i.slot)
            .collect();
        Ok(EquipmentRecompute {
            effective_attributes: stats.effective,
            resistances: stats.resistances,
            resources: stats.resources,
            mana_regeneration,
            spell_crit_percent,
            disabled_weapon_slots,
            modifier_steps,
            attribute_steps,
        })
    }
    /// Apply/remove only this worn item's admitted modifiers, in source order.
    /// The caller invokes this BEFORE committing durability across zero and must
    /// commit its full combat/equipment frame atomically with the returned recipe.
    /// Learning, slot allocation, current skills and passive state are untouched.
    pub fn set_item_broken(
        &mut self,
        slot: u8,
        broken: bool,
        live: Resources,
    ) -> Result<EquipmentRecompute> {
        let item = self
            .snapshot
            .items
            .iter()
            .find(|i| i.slot == slot)
            .cloned()
            .ok_or(InitError::Invalid("equipment break slot"))?;
        if item.max_durability == 0 {
            return Err(InitError::Invalid("nondurable item cannot break"));
        }
        let was_broken = self.snapshot.broken_slots.contains(&slot);
        if was_broken == broken {
            return self.current(live);
        }
        if self.snapshot.inactive_slots.contains(&slot) {
            return Err(InitError::Unsupported {
                domain: "inactive equipment durability mutation",
                id: item.item_id,
                detail: u32::from(slot),
            });
        }
        let mut next = self.clone();
        if broken {
            next.snapshot.broken_slots.push(slot);
            next.snapshot.broken_slots.sort_unstable();
        } else {
            next.snapshot.broken_slots.retain(|v| *v != slot);
        }
        let result = next.apply_item_modifiers(&item, if broken { -1 } else { 1 }, live)?;
        *self = next;
        Ok(result)
    }
    /// Change original armor membership, independently of durability. The host
    /// must authorize and atomically commit placement and replay history. This
    /// operation does not authorize restoring caller-supplied membership flags.
    pub fn set_item_active(
        &mut self,
        slot: u8,
        active: bool,
        live: Resources,
    ) -> Result<EquipmentRecompute> {
        let item = self
            .snapshot
            .items
            .iter()
            .find(|item| item.slot == slot)
            .cloned()
            .ok_or(InitError::Invalid("equipment membership slot"))?;
        if item.weapon || slot >= 15 {
            return Err(InitError::Invalid(
                "equipment membership requires original body armor",
            ));
        }
        let was_active = !self.snapshot.inactive_slots.contains(&slot);
        if was_active == active {
            return self.current(live);
        }
        let mut next = self.clone();
        if active {
            next.snapshot.inactive_slots.retain(|v| *v != slot);
        } else {
            next.snapshot.inactive_slots.push(slot);
            next.snapshot.inactive_slots.sort_unstable();
        }
        // Player::_ApplyItemMods returns before bonuses for broken items.
        let result = if next.snapshot.broken_slots.contains(&slot) {
            next.current(live)?
        } else {
            next.apply_item_modifiers(&item, if active { 1 } else { -1 }, live)?
        };
        *self = next;
        Ok(result)
    }
    /// Original pristine mainhand membership, distinct from armor and durability.
    /// The host owns exact custody, positive source Delay and weapon callbacks;
    /// this method applies only the ordered item modifier cells/resources.
    pub fn set_mainhand_active(
        &mut self,
        slot: u8,
        active: bool,
        live: Resources,
    ) -> Result<EquipmentRecompute> {
        let item = self
            .snapshot
            .items
            .iter()
            .find(|item| item.slot == slot)
            .cloned()
            .ok_or(InitError::Invalid("mainhand membership slot"))?;
        if slot != 15 || !item.weapon || self.snapshot.broken_slots.contains(&slot) {
            return Err(InitError::Invalid(
                "mainhand membership requires original unbroken weapon",
            ));
        }
        let was_active = !self.snapshot.inactive_slots.contains(&slot);
        if was_active == active {
            return self.current(live);
        }
        let mut next = self.clone();
        if active {
            next.snapshot.inactive_slots.retain(|value| *value != slot);
        } else {
            next.snapshot.inactive_slots.push(slot);
            next.snapshot.inactive_slots.sort_unstable();
        }
        let result = next.apply_item_modifiers(&item, if active { 1 } else { -1 }, live)?;
        *self = next;
        Ok(result)
    }
    fn apply_item_modifiers(
        &mut self,
        item: &EquipmentItemModifiers,
        sign: i32,
        live: Resources,
    ) -> Result<EquipmentRecompute> {
        let mut stats = Stats::restore(&self.snapshot.modifiers)?;
        self.validate_resources(&stats, live)?;
        stats.resources = live;
        let mut steps = vec![];
        let mut attribute_steps = vec![];
        for stat in item.stats {
            if stat.value == 0 {
                continue;
            }
            let amount = stat
                .value
                .checked_mul(sign)
                .ok_or(InitError::Numeric("item stat removal"))?;
            match stat.kind {
                0 => stats.resource(true, 0, amount)?,
                1 => stats.resource(false, 0, amount)?,
                3 => stats.attribute(1, 0, amount)?,
                4 => stats.attribute(0, 0, amount)?,
                5 => stats.attribute(3, 0, amount)?,
                6 => stats.attribute(4, 0, amount)?,
                7 => stats.attribute(2, 0, amount)?,
                _ => return Err(InitError::Invalid("recipe unknown item stat")),
            }
            stats.update()?;
            steps.push(stats.resources);
            let index = match stat.kind {
                3 => Some(1),
                4 => Some(0),
                5 => Some(3),
                6 => Some(4),
                7 => Some(2),
                _ => None,
            };
            if let Some(index) = index {
                attribute_steps.push((index, stats.effective));
            }
        }
        if item.armor != 0 {
            stats.resistance(0, 0, (item.armor as i32) * sign)?;
            stats.update()?;
            steps.push(stats.resources);
        }
        for (i, amount) in item.resistances.iter().enumerate() {
            if *amount != 0 {
                stats.resistance(i + 1, 0, (*amount as i32) * sign)?;
                stats.update()?;
                steps.push(stats.resources);
            }
        }
        let mut next = self.clone();
        next.snapshot.modifiers = stats.snapshot();
        let result = next.result(stats, steps, attribute_steps)?;
        *self = next;
        Ok(result)
    }
}

/// Sufficient static-modifier invariants for reloading the bounded new character:
/// passives are recast (not persisted), each percent cell has at most one source,
/// and integer flat-cell sums remain exact regardless of persisted load order.
pub(super) fn verify_first_entry_modifiers(
    input: &super::StarterInput,
    created: &super::InitializedMage,
) -> Result<()> {
    let mut flat = [0u64; 28];
    let mut percent = [0u8; 24];
    let mut add_flat = |cell: usize, value: i32| -> Result<()> {
        flat[cell] = flat[cell]
            .checked_add(u64::from(value.unsigned_abs()))
            .ok_or(InitError::Numeric("first-entry modifier sum"))?;
        if flat[cell] > 1_000_000 {
            return Err(InitError::Numeric(
                "first-entry exact additive modifier domain",
            ));
        }
        Ok(())
    };
    for aura in &created.initial_auras {
        let spell = input
            .spells
            .iter()
            .find(|s| s.id == aura.spell_id)
            .ok_or(super::missing("first-entry aura SpellEntry", aura.spell_id))?;
        if spell.words[6] & 0x40 == 0 {
            return Err(super::unsupported(
                "first-entry nonpassive aura reload",
                aura.spell_id,
                aura.aura_type,
            ));
        }
        match aura.aura_type {
            29 | 80 | 137 => {
                for stat in 0..5 {
                    if aura.misc_value < 0 || aura.misc_value == stat as i32 {
                        if aura.aura_type == 29 {
                            add_flat(5 + stat, aura.amount)?;
                        } else {
                            let cell = stat + if aura.aura_type == 137 { 5 } else { 0 };
                            percent[cell] = percent[cell].saturating_add(1);
                        }
                    }
                }
            }
            22 | 83 | 101 | 142 => {
                for school in 0..7 {
                    if aura.misc_value & (1 << school) != 0 {
                        if aura.aura_type == 22 || aura.aura_type == 83 {
                            add_flat(
                                10 + school + if aura.aura_type == 22 { 7 } else { 0 },
                                aura.amount,
                            )?;
                        } else {
                            let cell = 10 + school + if aura.aura_type == 101 { 7 } else { 0 };
                            percent[cell] = percent[cell].saturating_add(1);
                        }
                    }
                }
            }
            // Retained general reputation modifiers do not enter numeric stat cells.
            30 | 98 | 156 => {}
            _ => {
                return Err(super::unsupported(
                    "first-entry unknown aura",
                    aura.spell_id,
                    aura.aura_type,
                ))
            }
        }
    }
    for item in &created.stat_recipe.snapshot.items {
        for stat in item.stats {
            if stat.value != 0 {
                let cell = match stat.kind {
                    0 => 24,
                    1 => 25,
                    3 => 1,
                    4 => 0,
                    5 => 3,
                    6 => 4,
                    7 => 2,
                    _ => return Err(InitError::Invalid("first-entry item stat")),
                };
                add_flat(cell, stat.value)?;
            }
        }
        add_flat(10, item.armor as i32)?;
        for (school, resistance) in item.resistances.iter().enumerate() {
            add_flat(11 + school, *resistance as i32)?;
        }
    }
    if percent.iter().any(|n| *n > 1) {
        return Err(InitError::Invalid(
            "first-entry overlapping percent modifier order",
        ));
    }
    Ok(())
}
