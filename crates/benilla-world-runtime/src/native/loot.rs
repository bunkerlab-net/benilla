// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// LootMgr.cpp, LootHandler.cpp, Creature.cpp and Player.cpp. Copyright CMaNGOS
// contributors; upstream GPL-2.0-or-later, Rust GPL-3.0-or-later, without warranty.
use super::*;
use benilla_sim::{
    inventory as inv, loot as l, player_progression::Evidence, starter_init as init,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootCreatureRow {
    pub entry: u32,
    pub loot_id: u32,
    pub minimum_money: u32,
    pub maximum_money: u32,
    pub no_loot: bool,
    pub skinning_loot_id: u32,
    pub can_wield_loot: bool,
    pub can_be_multitapped: bool,
    pub is_pet: bool,
    pub despawn_instantly: bool,
    pub forever_corpse_duration: bool,
    pub corpse_acceleration_decay_ms: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootAudit {
    pub just_died_and_owner_hooks: Evidence,
    pub instance_battleground_outdoor_pvp_hooks: Evidence,
    pub death_scripts_linking_formation: Evidence,
    pub corpse_timer_overrides: Evidence,
    pub skinning_and_pickpocket: Evidence,
    pub looting_stealth_aura_callbacks: Evidence,
    pub group_and_controlled_units: Evidence,
}
#[derive(Clone, Debug)]
pub struct NativeLootInput {
    pub inventory: inv::Catalog,
    /// Explicit ordered creature_loot_template captures, never inferred from sorted SQL.
    pub captures: Vec<l::Capture>,
    /// Complete referenced leaf tables in their separate reference_loot_template namespace.
    pub reference_captures: Vec<l::Capture>,
    pub rules: l::Rules,
    pub creatures: Vec<LootCreatureRow>,
    pub audit: LootAudit,
}
#[derive(Clone, Debug)]
pub struct NativeLootGenesis {
    identity: crate::Digest,
    pub inventory: inv::Catalog,
    inventory_identity: crate::Digest,
    starter_source_identity: crate::Digest,
    pub starter_items: BTreeMap<u32, init::ItemRow>,
    /// Local handles for already-created source equipment, not upstream global GUIDs.
    equipment_instances: BTreeMap<u64, equipment::EquipmentOrigin>,
    pub(super) initial_inventory: inv::State,
    pub(super) catalog: l::Catalog,
    pub(super) rules: l::Rules,
    pub(super) creatures: BTreeMap<u32, LootCreatureRow>,
    audit: LootAudit,
}
impl PartialEq for NativeLootGenesis {
    fn eq(&self, o: &Self) -> bool {
        self.identity == o.identity
    }
}
impl Eq for NativeLootGenesis {}
#[cfg(test)]
mod registry_integrity_tests {
    use super::*;
    #[test]
    fn rehashed_registry_substitution_cannot_replace_source_genesis_objects() {
        let (mut profile, input, rows) = synthetic_loot_fixture::setup();
        profile.initialize_quest_progression(&input, rows).unwrap();
        let original = profile
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap();
        let mage = profile
            .combat
            .as_ref()
            .unwrap()
            .initialized_projection(&input)
            .unwrap();
        original.validate_immutable(&input, &mage).unwrap();
        for fault in 0..5 {
            let mut forged = original.clone();
            let (&id, &origin) = forged.equipment_instances.iter().next().unwrap();
            match fault {
                0 => {
                    forged
                        .equipment_instances
                        .get_mut(&id)
                        .unwrap()
                        .genesis_slot ^= 1;
                }
                1 => {
                    forged.equipment_instances.get_mut(&id).unwrap().item_id += 1;
                }
                2 => {
                    forged.equipment_instances.remove(&id);
                    forged.equipment_instances.insert(id + 1, origin);
                    forged.initial_inventory.next_item += 1;
                }
                3 => {
                    forged.equipment_instances.clear();
                }
                _ => {
                    forged.initial_inventory.next_item += 1;
                }
            }
            forged.identity = forged.reconstructed_identity();
            assert!(
                forged.validate_immutable(&input, &mage).is_err(),
                "fault {fault}"
            );
        }
    }
}
impl Serialize for NativeLootGenesis {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.identity.serialize(s)
    }
}
impl NativeLootGenesis {
    pub(super) fn build(
        profile: &Profile,
        input: &init::StarterInput,
        mage: &init::InitializedMage,
        rows: NativeLootInput,
    ) -> Result<Self, NativeError> {
        rows.inventory.validate()?;
        rows.rules.validate()?;
        if [
            &rows.audit.just_died_and_owner_hooks,
            &rows.audit.instance_battleground_outdoor_pvp_hooks,
            &rows.audit.death_scripts_linking_formation,
            &rows.audit.corpse_timer_overrides,
            &rows.audit.skinning_and_pickpocket,
            &rows.audit.looting_stealth_aura_callbacks,
            &rows.audit.group_and_controlled_units,
        ]
        .iter()
        .any(|e| **e != Evidence::Absent)
        {
            return Err(NativeError::UnsupportedMechanic(
                "ordinary corpse dependency closure",
            ));
        }
        let catalog = l::Catalog::new_with_references(rows.captures, rows.reference_captures)?;
        let mut creatures = BTreeMap::new();
        for r in rows.creatures {
            if r.entry == 0
                || r.skinning_loot_id != 0
                || r.can_wield_loot
                || r.can_be_multitapped
                || r.is_pet
                || r.despawn_instantly
                || r.forever_corpse_duration
                || r.corpse_acceleration_decay_ms == 0
                || r.corpse_acceleration_decay_ms > 86_400_000
                || r.loot_id != 0 && !catalog.contains(r.loot_id)
                || f64::from(r.maximum_money)
                    * f64::from(f32::from_bits(rows.rules.money_rate_bits))
                    > 1_000_000_000.0
            {
                return Err(NativeError::UnsupportedMechanic("ordinary corpse rows"));
            }
            if creatures.insert(r.entry, r).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        if creatures.keys().copied().collect::<BTreeSet<_>>()
            != profile.npcs.iter().map(|p| p.entry).collect()
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut shared_items = BTreeMap::new();
        for c in catalog.captures().chain(catalog.reference_captures()) {
            for lookup in c.items.values() {
                let l::ItemLookup::Present(item) = lookup else {
                    return Err(NativeError::InvalidProfile);
                };
                let i = rows.inventory.corpse_carried_ready(item.entry)?;
                if shared_items
                    .insert(item.entry, item)
                    .is_some_and(|previous| previous != item)
                {
                    return Err(NativeError::InvalidProfile);
                }
                if (
                    i.quality,
                    i.display_id,
                    i.flags,
                    i.start_quest,
                    i.extra_flags,
                    i.random_property,
                ) != (
                    item.quality,
                    item.display_id,
                    item.flags,
                    item.start_quest,
                    item.extra_flags,
                    item.random_property,
                ) {
                    return Err(NativeError::InvalidProfile);
                }
            }
        }
        let mut starter_items = BTreeMap::new();
        for row in &input.items {
            let item = rows.inventory.item(row.id)?;
            if item.class != row.class
                || item.inventory_type != row.inventory_type
                || item.flags != row.flags
                || item.stackable as u32 != row.stackable
                || item.maximum_count as u32 != row.unique_max_count
                || item.maximum_durability != row.max_durability
                || item.bag_family != row.bag_family
                || item.container_slots != row.container_slots
                || item.item_set != row.item_set
                || item.random_property != row.random_property
                || item.duration != row.duration
                || row.scripted
                || item
                    .spells
                    .iter()
                    .zip(&row.spells)
                    .any(|(a, b)| a.id != b.id || a.trigger != b.trigger)
            {
                return Err(NativeError::InvalidProfile);
            }
            starter_items.insert(row.id, row.clone());
        }
        let (initial_inventory, equipment_instances) =
            Self::initial_objects(mage, &rows.inventory)?;
        let mut genesis = Self {
            identity: [0; 32],
            inventory_identity: rows.inventory.hash(),
            starter_source_identity: input.source_identity(),
            inventory: rows.inventory,
            starter_items,
            equipment_instances,
            initial_inventory,
            catalog,
            rules: rows.rules,
            creatures,
            audit: rows.audit,
        };
        genesis.identity = genesis.reconstructed_identity();
        Ok(genesis)
    }
    pub fn identity(&self) -> crate::Digest {
        self.identity
    }
    /// A registered handle names the same immutable equipment origin even when
    /// that object is broken. This does not authorize changing its placement.
    pub fn equipment_origin(&self, instance: u64) -> Option<equipment::EquipmentOrigin> {
        self.equipment_instances.get(&instance).copied()
    }
    pub fn equipment_instance(&self, origin: equipment::EquipmentOrigin) -> Option<u64> {
        self.equipment_instances
            .iter()
            .find_map(|(&id, &registered)| (registered == origin).then_some(id))
    }
    pub fn equipment_instances(
        &self,
    ) -> impl Iterator<Item = (u64, equipment::EquipmentOrigin)> + '_ {
        self.equipment_instances
            .iter()
            .map(|(&id, &origin)| (id, origin))
    }
    fn initial_objects(
        mage: &init::InitializedMage,
        catalog: &inv::Catalog,
    ) -> Result<(inv::State, BTreeMap<u64, equipment::EquipmentOrigin>), NativeError> {
        let mut equipped = BTreeMap::<u32, u32>::new();
        for item in mage.equipped.values() {
            *equipped.entry(item.item).or_default() += 1;
        }
        let mut inventory = inv::State::new(equipped, 1)?;
        // Keep the existing local backpack creation order and handles. Register
        // the already-created worn objects afterward; this is not CloneItem or
        // a claim about the source server's global low-GUID allocation order.
        for stack in &mage.inventory {
            inventory.store_new(catalog, stack.item, stack.count, |_, _, _| Ok(()))?;
        }
        let mut registered = BTreeMap::new();
        for (&slot, item) in &mage.equipped {
            let id = inventory.next_item;
            inventory.next_item = id.checked_add(1).ok_or(NativeError::InvalidProfile)?;
            registered.insert(
                id,
                equipment::EquipmentOrigin {
                    genesis_slot: slot,
                    item_id: item.item,
                },
            );
        }
        inventory.validate(catalog)?;
        Ok((inventory, registered))
    }
    fn reconstructed_identity(&self) -> crate::Digest {
        crate::digest(
            b"benilla-native-loot-genesis-reconstruction-v4\0",
            &(
                inv::CORPSE_POLICY_ID,
                self.inventory.hash(),
                self.catalog.identity(),
                &self.rules,
                &self.creatures,
                &self.audit,
                &self.initial_inventory,
                &self.equipment_instances,
                self.starter_source_identity,
            ),
        )
    }
    pub(super) fn validate_immutable(
        &self,
        input: &init::StarterInput,
        mage: &init::InitializedMage,
    ) -> Result<(), NativeError> {
        if self.identity != self.reconstructed_identity()
            || self.inventory.hash() != self.inventory_identity
            || self.starter_source_identity != input.source_identity()
            || self.starter_items != input.items.iter().map(|i| (i.id, i.clone())).collect()
        {
            return Err(NativeError::InvalidProfile);
        }
        self.inventory.validate()?;
        self.rules.validate()?;
        self.initial_inventory.validate(&self.inventory)?;
        let (inventory, registered) = Self::initial_objects(mage, &self.inventory)?;
        if inventory != self.initial_inventory || registered != self.equipment_instances {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseLoot {
    pub owner: crate::PlayerId,
    pub corpse: crate::NpcId,
    pub death_at_ms: u64,
    pub first_counter: u64,
    pub end_counter: u64,
    pub generated: l::Generated,
    pub collected: BTreeSet<u8>,
    pub money_taken: bool,
    pub opened: bool,
    pub looted: bool,
    pub draws: Vec<Draw>,
}
impl CorpseLoot {
    pub fn items(&self) -> impl Iterator<Item = &l::LootItem> {
        self.generated
            .items
            .iter()
            .filter(|i| !self.collected.contains(&i.slot))
    }
    /// Quest-only slots also require current need, LootItem::AllowedForPlayer at LootMgr.cpp:479-483.
    pub fn visible_items<'a>(
        &'a self,
        owner: &'a BTreeSet<u32>,
    ) -> impl Iterator<Item = &'a l::LootItem> {
        self.items()
            .filter(move |item| !item.quest_only || owner.contains(&item.item))
    }
    pub fn money(&self) -> u32 {
        if self.money_taken {
            0
        } else {
            self.generated.money
        }
    }
    pub fn empty(&self) -> bool {
        self.items().next().is_none() && self.money() == 0
    }
    pub fn lootable(&self) -> bool {
        !self.looted && (!self.empty() || self.generated.fake && !self.opened)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootWindow {
    pub corpse: crate::NpcId,
    pub items: Vec<l::LootItem>,
    pub money: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum LootEvent {
    Opened(LootWindow),
    Released {
        corpse: crate::NpcId,
        send_response: bool,
    },
    ItemRemoved {
        corpse: crate::NpcId,
        slot: u8,
    },
    MoneyRemoved {
        corpse: crate::NpcId,
    },

    StorageFailed {
        corpse: crate::NpcId,
        item: u32,
        result: inv::InventoryResult,
    },
}
/// SendNewItem's source call semantics, independent of corpse-window events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ItemAcquisition {
    CorpseLoot,
    AdminSelf,
}
/// One recomputable bounded witness for a currently live stack. The previous
/// digest commits history; reason commits its latest operation; digest binds the
/// actual current slot and full instance. It is not an owner-authenticity proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemReceipt {
    pub previous: Option<crate::Digest>,
    pub reason: crate::Digest,
    pub digest: crate::Digest,
}
impl ItemReceipt {
    pub(in crate::native) fn new(
        previous: Option<crate::Digest>,
        reason: crate::Digest,
        slot: u8,
        item: &inv::Instance,
    ) -> Self {
        let digest = crate::digest(
            b"benilla-native-item-chain-v2\0",
            &(previous, reason, slot, item),
        );
        Self {
            previous,
            reason,
            digest,
        }
    }
    pub(in crate::native) fn validates(&self, slot: u8, item: &inv::Instance) -> bool {
        !self.previous.is_some_and(|v| v == [0; 32])
            && self.digest == Self::new(self.previous, self.reason, slot, item).digest
    }
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(super) fn initial_inventory_receipts(i: &inv::State) -> BTreeMap<u64, ItemReceipt> {
    i.slots
        .iter()
        .map(|(&slot, item)| (item.id, ItemReceipt::new(None, [0; 32], slot, item)))
        .collect()
}
/// Removal drops a witness; a mutation retains only its immediate prior digest.
pub(super) fn update_inventory_receipts(
    q: &mut QuestProgressionState,
    reason: crate::Digest,
) -> Result<(), NativeError> {
    let i = q.inventory.as_ref().ok_or(NativeError::InvalidState)?;
    q.inventory_receipts = i
        .slots
        .iter()
        .map(|(&slot, item)| {
            (
                item.id,
                ItemReceipt::new(
                    q.inventory_receipts.get(&item.id).map(|r| r.digest),
                    reason,
                    slot,
                    item,
                ),
            )
        })
        .collect();
    Ok(())
}
pub(super) struct RandomHost<'a> {
    pub state: &'a mut State,
    pub corpse: crate::NpcId,
    pub draws: &'a mut Vec<Draw>,
}
impl l::Random for RandomHost<'_> {
    fn inclusive(&mut self, low: u32, high: u32) -> Result<u32, l::Error> {
        self.state
            .draw_range(
                0,
                DrawPurpose::Loot {
                    corpse: self.corpse,
                },
                low.into(),
                high.into(),
                self.draws,
            )
            .map(|n| n as u32)
            .map_err(|_| l::Error::Draw)
    }
    fn chance(&mut self) -> Result<f64, l::Error> {
        self.state
            .draw_range(
                0,
                DrawPurpose::LootChance {
                    corpse: self.corpse,
                },
                0,
                (1i64 << 53) - 1,
                self.draws,
            )
            .map(|n| (n as f64 / (1u64 << 53) as f64) * 100.0)
            .map_err(|_| l::Error::Draw)
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<(), l::Error> {
        for i in (1..values.len()).rev() {
            let j = self
                .state
                .draw_range(
                    0,
                    DrawPurpose::LootShuffle {
                        corpse: self.corpse,
                    },
                    0,
                    i as i64,
                    self.draws,
                )
                .map_err(|_| l::Error::Draw)?;
            values.swap(i, j as usize);
        }
        Ok(())
    }
}
impl From<inv::Error> for NativeError {
    fn from(e: inv::Error) -> Self {
        Self::Inventory(e)
    }
}
impl From<l::Error> for NativeError {
    fn from(e: l::Error) -> Self {
        Self::Loot(e)
    }
}
impl NativeWorld {
    pub fn loot_quest_items(&self) -> Result<BTreeSet<u32>, NativeError> {
        self.profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .loot_quest_items(
                self.state
                    .quest_progression
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?,
            )
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn validate_loot_state(&self) -> Result<(), NativeError> {
        let genesis = self
            .profile
            .quest_progression
            .as_ref()
            .and_then(|g| g.loot.as_ref());
        for npc in self.state.npcs.values() {
            if npc
                .original_loot_recipient
                .is_some_and(|p| p != self.state.player)
                || (matches!(npc.ai.phase(), ai::Phase::Idle | ai::Phase::ReturningHome)
                    && npc.original_loot_recipient.is_some())
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            let row = genesis.map(|g| &g.creatures[&self.profile.npc(npc.id.spawn).unwrap().entry]);
            match (&npc.corpse_loot, genesis, row) {
                (Some(c), Some(g), Some(row)) => {
                    if row.no_loot
                        || npc.ai.phase() != ai::Phase::Corpse
                        || npc.health != 0
                        || c.corpse != npc.id
                        || c.owner != self.state.player
                        || npc.original_loot_recipient != Some(c.owner)
                        || npc.ai.death_at_ms() != Some(c.death_at_ms)
                        || c.first_counter
                            < creature_generation_binding::validate_prefix(
                                &self.profile,
                                &self.state,
                                self.state.initial_cast_genesis.end_counter(),
                            )?
                        || c.first_counter > c.end_counter
                        || c.end_counter > self.state.rng_counter
                        || c.draws.len() > 4096
                        || c.generated.items.len() > 16
                        || c.collected
                            .iter()
                            .any(|slot| !c.generated.items.iter().any(|item| item.slot == *slot))
                        || c.looted && !(c.empty() || c.generated.fake)
                        || npc
                            .ai
                            .corpse_timer_calls()
                            .iter()
                            .any(|call| call.delay_ms != row.corpse_acceleration_decay_ms)
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    let mut probe = self.state.clone();
                    probe.rng_counter = c.first_counter;
                    let mut draws = vec![];
                    let expected = g.catalog.generate(
                        row.loot_id,
                        row.minimum_money,
                        row.maximum_money,
                        &g.rules,
                        &c.generated.owner_quests,
                        &mut RandomHost {
                            state: &mut probe,
                            corpse: npc.id,
                            draws: &mut draws,
                        },
                    )?;
                    if expected != c.generated
                        || probe.rng_counter != c.end_counter
                        || draws != c.draws
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                }
                (None, Some(_), Some(row))
                    if npc.ai.phase() == ai::Phase::Corpse && !row.no_loot =>
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                (None, _, _) => {}
                _ => return Err(NativeError::InvalidCheckpoint),
            }
        }
        if let Some(id) = self
            .state
            .quest_progression
            .as_ref()
            .and_then(|q| q.loot_target)
        {
            if !self.state.npcs.get(&id.spawn).is_some_and(|n| {
                n.id == id
                    && n.corpse_loot
                        .as_ref()
                        .is_some_and(|l| l.opened && l.owner == self.state.player)
            }) {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        Ok(())
    }
}
