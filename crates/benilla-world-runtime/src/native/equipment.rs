// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Original GPL-2.0-or-later source grant; Rust translation GPL-3.0-or-later.
//! Checked equipment state for the selected-hit and NPC-death durability callbacks.
//! Source branches, integer binary64 proof and integration obligations: EQUIPMENT.md.
//! This module draws no RNG and does not choose whether a death qualifies for loss.
//! A successful callback must recompute every reported broken item's stat effects;
//! rejection leaves this equipment state unchanged. The enclosing world transaction
//! must stage the callback's other state too, including health/mana and weapon use.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EQUIPMENT_FORMAT_VERSION: u32 = 1;
pub const EQUIPMENT_POLICY: &str = "cmangos-8ec338a1-equipped-durability-soft-binary64-v1";
/// Player.h's [EQUIPMENT_SLOT_START, EQUIPMENT_SLOT_END), excluding inventory bags.
pub const EQUIPMENT_SLOT_END: u8 = 19;
/// The source point-loss callback converts both durability fields to signed int32.
pub const MAX_DURABILITY: u32 = i32::MAX as u32;

/// Inert worn-item data. The item template and maximum must come from the admitted
/// source loadout; missing durability fields must never be synthesized as zero.
/// Slot plus template ID identifies an item in this no-equip/no-inventory domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquippedItem {
    pub slot: u8,
    pub item_id: u32,
    pub max_durability: u32,
    pub current_durability: u32,
}
impl EquippedItem {
    /// Non-durable equipment (max == 0) still contributes its ordinary bonuses.
    pub fn is_broken(&self) -> bool {
        self.max_durability > 0 && self.current_durability == 0
    }
    fn validate(&self) -> Result<(), EquipmentError> {
        if self.slot >= EQUIPMENT_SLOT_END {
            return Err(EquipmentError::InvalidSlot(self.slot));
        }
        if self.item_id == 0 {
            return Err(EquipmentError::InvalidItem(self.slot));
        }
        if self.max_durability > MAX_DURABILITY || self.current_durability > self.max_durability {
            return Err(EquipmentError::InvalidDurability(self.slot));
        }
        Ok(())
    }
}

/// Public, inert checkpoint data. Deserializing it grants no live-state authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSnapshot {
    pub version: u32,
    #[serde(deserialize_with = "deserialize_items")]
    pub items: Vec<EquippedItem>,
}

fn deserialize_items<'de, D>(deserializer: D) -> Result<Vec<EquippedItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Items;
    impl<'de> serde::de::Visitor<'de> for Items {
        type Value = Vec<EquippedItem>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 19 equipped items")
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            if seq
                .size_hint()
                .is_some_and(|n| n > usize::from(EQUIPMENT_SLOT_END))
            {
                return Err(serde::de::Error::custom("too many equipped items"));
            }
            let mut items = Vec::with_capacity(usize::from(EQUIPMENT_SLOT_END));
            while let Some(item) = seq.next_element()? {
                if items.len() == usize::from(EQUIPMENT_SLOT_END) {
                    return Err(serde::de::Error::custom("too many equipped items"));
                }
                items.push(item);
            }
            Ok(items)
        }
    }
    deserializer.deserialize_seq(Items)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentError {
    IncompatibleVersion,
    TooManyItems,
    InvalidSlot(u8),
    InvalidItem(u8),
    InvalidDurability(u8),
    UnsupportedMaximum(u32),
    NonCanonicalSlots,
    LoadoutMismatch,
}
impl std::fmt::Display for EquipmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native equipment: {self:?}")
    }
}
impl std::error::Error for EquipmentError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityChange {
    pub slot: u8,
    pub item_id: u32,
    pub max_durability: u32,
    pub old_durability: u32,
    pub new_durability: u32,
}

/// CMaNGOS removes the item's modifiers before setting its durability to zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EquipmentStatTransition {
    Broken { slot: u8, item_id: u32 },
}

/// Both lists are in ascending equipment-slot order, as in DurabilityLossAll.
#[must_use = "journal durability changes and apply every item-stat transition"]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityChanges {
    pub changes: Vec<DurabilityChange>,
    pub stat_transitions: Vec<EquipmentStatTransition>,
}

/// Validated live state. No Deserialize implementation can bypass checked restore.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentState {
    items: BTreeMap<u8, EquippedItem>,
}
impl EquipmentState {
    /// Validate explicit initial data or an inert snapshot structurally. Loading
    /// saved state also requires `restore` against the authoritative loadout.
    pub fn from_snapshot(snapshot: EquipmentSnapshot) -> Result<Self, EquipmentError> {
        if snapshot.version != EQUIPMENT_FORMAT_VERSION {
            return Err(EquipmentError::IncompatibleVersion);
        }
        if snapshot.items.len() > usize::from(EQUIPMENT_SLOT_END) {
            return Err(EquipmentError::TooManyItems);
        }
        for item in &snapshot.items {
            item.validate()?;
        }
        if snapshot
            .items
            .windows(2)
            .any(|items| items[0].slot >= items[1].slot)
        {
            return Err(EquipmentError::NonCanonicalSlots);
        }
        Ok(Self {
            items: snapshot
                .items
                .into_iter()
                .map(|item| (item.slot, item))
                .collect(),
        })
    }

    /// Reject additions, omissions and changes to source item IDs/maxima. The
    /// authoritative initial current values need not equal the saved current values.
    /// Restore the stat recipe from the resulting broken flags in the same enclosing
    /// transaction; this helper does not claim to validate any saved effective stats.
    pub fn restore(
        snapshot: EquipmentSnapshot,
        authoritative_loadout: &Self,
    ) -> Result<Self, EquipmentError> {
        let restored = Self::from_snapshot(snapshot)?;
        if restored.items.len() != authoritative_loadout.items.len()
            || restored.items.iter().any(|(slot, item)| {
                authoritative_loadout
                    .items
                    .get(slot)
                    .is_none_or(|expected| {
                        item.item_id != expected.item_id
                            || item.max_durability != expected.max_durability
                    })
            })
        {
            return Err(EquipmentError::LoadoutMismatch);
        }
        Ok(restored)
    }

    pub fn snapshot(&self) -> EquipmentSnapshot {
        EquipmentSnapshot {
            version: EQUIPMENT_FORMAT_VERSION,
            items: self.items.values().copied().collect(),
        }
    }

    pub fn item(&self, slot: u8) -> Option<&EquippedItem> {
        self.items.get(&slot)
    }

    pub fn items(&self) -> impl ExactSizeIterator<Item = &EquippedItem> {
        self.items.values()
    }

    /// Player::DurabilityPointLossForEquipSlot after the caller's chance and slot
    /// rolls. Empty, already-broken and non-durable selected slots are source no-ops.
    /// Every actual change is staged and passed to `recompute` before self commits.
    pub fn apply_hit_loss<E, F>(&mut self, slot: u8, recompute: F) -> Result<DurabilityChanges, E>
    where
        E: From<EquipmentError>,
        F: FnOnce(&EquipmentSnapshot, &EquipmentSnapshot, &DurabilityChanges) -> Result<(), E>,
    {
        if slot >= EQUIPMENT_SLOT_END {
            return Err(EquipmentError::InvalidSlot(slot).into());
        }
        self.apply_with_recompute(|item| u32::from(item.slot == slot), recompute)
    }

    /// Unit::Kill's qualifying NPC death branch: DurabilityLossAll(0.10f, false).
    /// The caller proves durabilityLoss && !responsiblePlayer && !InBattleGround
    /// and that the killing spell does not have NO_DURABILITY_LOSS. No inventory,
    /// PvP classification, resurrection loss or player-bot override is supplied here.
    pub fn apply_npc_death_loss<E, F>(&mut self, recompute: F) -> Result<DurabilityChanges, E>
    where
        E: From<EquipmentError>,
        F: FnOnce(&EquipmentSnapshot, &EquipmentSnapshot, &DurabilityChanges) -> Result<(), E>,
    {
        self.apply_with_recompute(|item| death_loss_unchecked(item.max_durability), recompute)
    }

    fn apply_with_recompute<E, F>(
        &mut self,
        points: impl Fn(&EquippedItem) -> u32,
        recompute: F,
    ) -> Result<DurabilityChanges, E>
    where
        F: FnOnce(&EquipmentSnapshot, &EquipmentSnapshot, &DurabilityChanges) -> Result<(), E>,
    {
        let mut next = self.clone();
        let mut result = DurabilityChanges {
            changes: vec![],
            stat_transitions: vec![],
        };
        for item in next.items.values_mut() {
            let old = item.current_durability;
            // The admitted nonnegative points cannot overflow signed source subtraction.
            let new = old.saturating_sub(points(item));
            if old == new {
                continue;
            }
            result.changes.push(DurabilityChange {
                slot: item.slot,
                item_id: item.item_id,
                max_durability: item.max_durability,
                old_durability: old,
                new_durability: new,
            });
            if new == 0 {
                result
                    .stat_transitions
                    .push(EquipmentStatTransition::Broken {
                        slot: item.slot,
                        item_id: item.item_id,
                    });
            }
            item.current_durability = new;
        }
        if !result.changes.is_empty() {
            recompute(&self.snapshot(), &next.snapshot(), &result)?;
            *self = next;
        }
        Ok(result)
    }
}

/// Exact source uint32(max * double(0.10f)), with its minimum-one branch.
/// All arithmetic is integer-only, including the binary64 rounding operation.
pub fn npc_death_loss_points(max_durability: u32) -> Result<u32, EquipmentError> {
    if max_durability > MAX_DURABILITY {
        return Err(EquipmentError::UnsupportedMaximum(max_durability));
    }
    Ok(death_loss_unchecked(max_durability))
}

fn death_loss_unchecked(max: u32) -> u32 {
    if max == 0 {
        return 0;
    }
    // 0.10f (0x3dcccccd) promotes exactly to binary64 0x3fb99999a0000000,
    // the rational 13421773 / 2^27. max converts exactly; their product has at
    // most 55 significant bits. Round to binary64's 53, ties-to-even, BEFORE
    // truncating. A raw rational floor alone is incorrect near large integers.
    let product = u64::from(max) * 13_421_773;
    let discarded_bits = (u64::BITS - product.leading_zeros()).saturating_sub(53);
    let rounded = if discarded_bits == 0 {
        product
    } else {
        let significand = product >> discarded_bits;
        let discarded = product & ((1 << discarded_bits) - 1);
        let halfway = 1 << (discarded_bits - 1);
        let increment = discarded > halfway || (discarded == halfway && significand & 1 != 0);
        (significand + u64::from(increment)) << discarded_bits
    };
    ((rounded >> 27) as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(slot: u8, max: u32, current: u32) -> EquippedItem {
        EquippedItem {
            slot,
            item_id: 100 + u32::from(slot),
            max_durability: max,
            current_durability: current,
        }
    }
    fn snapshot(items: Vec<EquippedItem>) -> EquipmentSnapshot {
        EquipmentSnapshot {
            version: EQUIPMENT_FORMAT_VERSION,
            items,
        }
    }
    fn state(items: Vec<EquippedItem>) -> EquipmentState {
        EquipmentState::from_snapshot(snapshot(items)).unwrap()
    }
    fn accept(
        _before: &EquipmentSnapshot,
        _after: &EquipmentSnapshot,
        _changes: &DurabilityChanges,
    ) -> Result<(), EquipmentError> {
        Ok(())
    }

    #[test]
    fn selected_slot_loses_one_and_break_callback_observes_both_states() {
        let mut equipment = state(vec![item(0, 10, 10), item(15, 20, 1)]);
        let changes = equipment
            .apply_hit_loss(15, |before, after, changes| {
                assert_eq!(before.items[1].current_durability, 1);
                assert_eq!(after.items[1].current_durability, 0);
                assert_eq!(
                    changes.stat_transitions,
                    vec![EquipmentStatTransition::Broken {
                        slot: 15,
                        item_id: 115
                    }]
                );
                Ok::<(), EquipmentError>(())
            })
            .unwrap();
        assert_eq!(changes.changes.len(), 1);
        assert_eq!(equipment.item(0).unwrap().current_durability, 10);
        assert!(equipment.item(15).unwrap().is_broken());
        assert_eq!(
            equipment.apply_hit_loss(15, accept).unwrap().changes.len(),
            0
        );
    }

    #[test]
    fn no_op_slots_do_not_call_stat_recompute() {
        let mut equipment = state(vec![item(0, 0, 0), item(1, 100, 0)]);
        for slot in [0, 1, 18] {
            let changes = equipment
                .apply_hit_loss::<EquipmentError, _>(slot, |_, _, _| {
                    panic!("a no-op must not call recompute")
                })
                .unwrap();
            assert!(changes.changes.is_empty());
            assert!(changes.stat_transitions.is_empty());
        }
        assert!(!equipment.item(0).unwrap().is_broken());
        assert!(equipment.item(1).unwrap().is_broken());
    }

    #[test]
    fn death_uses_maximum_minimum_one_clamp_and_slot_order() {
        let mut equipment = state(vec![
            item(0, 0, 0),
            item(3, 9, 5),
            item(4, 100, 8),
            item(15, 100, 100),
            item(18, 100, 0),
        ]);
        let result = equipment.apply_npc_death_loss(accept).unwrap();
        assert_eq!(
            result
                .changes
                .iter()
                .map(|c| (c.slot, c.old_durability, c.new_durability))
                .collect::<Vec<_>>(),
            vec![(3, 5, 4), (4, 8, 0), (15, 100, 90)]
        );
        assert_eq!(
            result.stat_transitions,
            vec![EquipmentStatTransition::Broken {
                slot: 4,
                item_id: 104
            }]
        );
        assert_eq!(equipment.item(0).unwrap().current_durability, 0);
    }

    #[test]
    fn failed_recompute_rolls_back_all_equipment_changes() {
        let mut equipment = state(vec![item(0, 100, 1), item(4, 100, 2), item(15, 100, 99)]);
        let before = equipment.clone();
        let result = equipment.apply_npc_death_loss(|old, next, changes| {
            assert_eq!(old.items[0].current_durability, 1);
            assert_eq!(next.items[0].current_durability, 0);
            assert_eq!(changes.stat_transitions.len(), 2);
            Err(EquipmentError::LoadoutMismatch)
        });
        assert_eq!(result, Err(EquipmentError::LoadoutMismatch));
        assert_eq!(equipment, before);
        assert_eq!(
            equipment.apply_hit_loss(19, accept),
            Err(EquipmentError::InvalidSlot(19))
        );
        assert_eq!(equipment, before);
    }

    #[test]
    fn validation_rejects_invalid_slots_identity_counts_bounds_and_order() {
        for bad in [
            item(19, 1, 1),
            item(255, 1, 1),
            item(1, 0, 1),
            item(1, 2, 3),
            item(1, MAX_DURABILITY + 1, 0),
            EquippedItem {
                item_id: 0,
                ..item(1, 1, 1)
            },
        ] {
            assert!(EquipmentState::from_snapshot(snapshot(vec![bad])).is_err());
        }
        assert_eq!(
            EquipmentState::from_snapshot(snapshot(vec![item(0, 0, 0); 20])),
            Err(EquipmentError::TooManyItems)
        );
        for slots in [[1, 1], [2, 1]] {
            assert_eq!(
                EquipmentState::from_snapshot(snapshot(
                    slots.map(|slot| item(slot, 1, 1)).to_vec()
                )),
                Err(EquipmentError::NonCanonicalSlots)
            );
        }
        let mut bad = snapshot(vec![]);
        bad.version += 1;
        assert_eq!(
            EquipmentState::from_snapshot(bad),
            Err(EquipmentError::IncompatibleVersion)
        );
        assert!(npc_death_loss_points(MAX_DURABILITY + 1).is_err());
    }

    #[test]
    fn all_nineteen_slots_are_admitted_and_template_ids_may_repeat() {
        let equipment = state(
            (0..19)
                .map(|slot| EquippedItem {
                    item_id: 123,
                    ..item(slot, 10, 10)
                })
                .collect(),
        );
        assert_eq!(equipment.items().len(), 19);
        let bytes = serde_json::to_vec(&equipment.snapshot()).unwrap();
        let inert = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            EquipmentState::restore(inert, &equipment).unwrap(),
            equipment
        );
    }

    #[test]
    fn checked_restore_binds_slot_item_and_maximum_but_preserves_loss() {
        let initial = state(vec![item(0, 100, 100), item(15, 10, 10)]);
        let mut damaged = initial.clone();
        let _changes = damaged.apply_npc_death_loss(accept).unwrap();
        let bytes = serde_json::to_vec(&damaged.snapshot()).unwrap();
        let restored =
            EquipmentState::restore(serde_json::from_slice(&bytes).unwrap(), &initial).unwrap();
        assert_eq!(restored, damaged);
        for field in 0..4 {
            let mut tampered = damaged.snapshot();
            match field {
                0 => tampered.items[0].slot = 1,
                1 => tampered.items[0].item_id += 1,
                2 => tampered.items[0].max_durability += 1,
                _ => {
                    tampered.items.pop();
                }
            }
            assert_eq!(
                EquipmentState::restore(tampered, &initial),
                Err(EquipmentError::LoadoutMismatch)
            );
        }
    }

    #[test]
    fn serde_rejects_missing_source_fields_unknown_fields_and_oversize_items() {
        let data = serde_json::to_value(snapshot(vec![item(0, 10, 9)])).unwrap();
        for field in ["version", "items"] {
            let mut missing = data.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<EquipmentSnapshot>(missing).is_err());
        }
        for field in ["slot", "item_id", "max_durability", "current_durability"] {
            let mut missing = data.clone();
            missing["items"][0].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<EquipmentSnapshot>(missing).is_err());
        }
        let mut unknown = data.clone();
        unknown["items"][0]["default_max"] = 1.into();
        assert!(serde_json::from_value::<EquipmentSnapshot>(unknown).is_err());
        let mut unknown = data;
        unknown["policy_override"] = true.into();
        assert!(serde_json::from_value::<EquipmentSnapshot>(unknown).is_err());
        let too_many = serde_json::to_vec(&snapshot(vec![item(0, 1, 1); 20])).unwrap();
        assert!(serde_json::from_slice::<EquipmentSnapshot>(&too_many).is_err());
    }

    #[test]
    fn integer_binary64_matches_source_order_oracle_vectors() {
        // Independent C++ oracle: double percent = 0.10f;
        // max ? std::max(uint32_t(1), uint32_t(max * percent)) : 0.
        // Large witnesses distinguish binary64 rounding from exact rational floor.
        for (max, expected) in [
            (0, 0),
            (1, 1),
            (9, 1),
            (10, 1),
            (19, 1),
            (20, 2),
            (100, 10),
            (1_000_000, 100_000),
            (738_197_499, 73_819_751),
            (1_342_177_270, 134_217_729),
            (2_147_483_638, 214_748_367),
            (2_147_483_647, 214_748_367),
        ] {
            assert_eq!(npc_death_loss_points(max).unwrap(), expected, "max={max}");
        }
        for max in 1..=100_000 {
            assert_eq!(npc_death_loss_points(max).unwrap(), (max / 10).max(1));
        }
    }

    #[test]
    fn full_domain_samples_match_independent_cpp_oracle_digest() {
        use sha2::{Digest, Sha256};
        let mut oracle = Sha256::new();
        let mut seed = 0x65ab_c139_u64;
        for _ in 0..100_000 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let max = (seed >> 33) as u32;
            oracle.update(max.to_le_bytes());
            oracle.update(npc_death_loss_points(max).unwrap().to_le_bytes());
        }
        // SHA-256 of little-endian (max, source_loss) pairs from the independent
        // C++ source-order oracle reproduced in EQUIPMENT.md, not this helper.
        let digest: [u8; 32] = oracle.finalize().into();
        assert_eq!(
            digest,
            [
                0x97, 0xf5, 0x6c, 0x5d, 0x2c, 0xc1, 0x0f, 0xcf, 0x16, 0x70, 0x8a, 0x35, 0x97, 0x5a,
                0xca, 0x00, 0x30, 0xb5, 0x5e, 0xc8, 0x7a, 0x47, 0xe7, 0xc3, 0xc9, 0x7f, 0x28, 0x13,
                0xa8, 0x2c, 0xac, 0x7f,
            ]
        );
    }
}
