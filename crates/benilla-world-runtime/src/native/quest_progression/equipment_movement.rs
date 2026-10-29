// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Player::SwapItem, RemoveItem and EquipItem.
//! Original armor custody; no creation, replacement, repairs or native admission.
use super::*;
use crate::native::equipment::{EquipmentOrigin, EquipmentSnapshot, EquipmentState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentMoveRequest {
    pub source: u8,
    pub destination: u8,
}

/// The source handlers have different guard order and destination selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EquipmentMoveOperation {
    Swap,
    AutoEquip,
}

/// Owner-captured source operands establish a reachable context, not an
/// authenticated chronology. Whole TickRecord replay supplies the latter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct MoveContext {
    pub operation: EquipmentMoveOperation,
    pub world: crate::WorldId,
    pub player: crate::PlayerId,
    pub profile: crate::Digest,
    pub sequence: u64,
    pub captured_ms: u64,
    pub in_combat: bool,
    // Queried only when equipping. Unequip never checks IsNonMeleeSpellCasted.
    #[serde(deserialize_with = "crate::native::creature_collision_width::required_option")]
    pub generic_slot: Option<player_melee::casting::GenericSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentMoveReceipt {
    pub request: EquipmentMoveRequest,
    pub instance: u64,
    pub origin: EquipmentOrigin,
    pub before: inv::State,
    pub after: inv::State,
    pub before_receipts: BTreeMap<u64, ItemReceipt>,
    pub after_receipts: BTreeMap<u64, ItemReceipt>,
    pub(in crate::native) context: MoveContext,
    pub(in crate::native) before_placements: Placements,
}

/// Permanent original-handle custody. Retired handles cannot be reused or revived.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EquipmentPlacement {
    Slot(u8),
    Retired,
}
pub(in crate::native) type Placements = BTreeMap<u64, EquipmentPlacement>;

fn loot(g: &QuestProgressionGenesis) -> Result<&NativeLootGenesis, NativeError> {
    g.loot
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic("armor item registry"))
}

pub(in crate::native) fn initial_placements(g: &QuestProgressionGenesis) -> Placements {
    g.loot
        .as_ref()
        .into_iter()
        .flat_map(|l| l.equipment_instances())
        .map(|(id, origin)| (id, EquipmentPlacement::Slot(origin.genesis_slot)))
        .collect()
}

/// The full spell-authority owner separately closes equip spells, enchantments,
/// item-dependent casts and control hooks. These checks bound the moved object.
fn source_item(
    g: &QuestProgressionGenesis,
    origin: EquipmentOrigin,
) -> Result<&inv::Item, NativeError> {
    let l = loot(g)?;
    let row = l
        .starter_items
        .get(&origin.item_id)
        .ok_or(NativeError::InvalidState)?;
    let item = l.inventory.grant_ready(origin.item_id)?;
    if origin.genesis_slot >= 15
        || row.class != 4
        || item.class != 4
        || row.required_skill != 0
        || row.required_spell != 0
        || row.required_honor_rank != 0
        || row.required_reputation_faction != 0
        || row.required_level > 1
        || row.block != 0
        || row.item_set != 0
        || row.random_property != 0
        || row.duration != 0
        || row.scripted
        || row.spells.iter().any(|s| s.id != 0)
        || item.spells.iter().any(|s| s.id != 0)
        || item.stackable != 1
        || !matches!(item.bonding, 0 | 1 | 4)
        || item.container_slots != 0
        || item.bag_family != 0
    {
        return Err(NativeError::UnsupportedMechanic(
            "original pristine armor movement subset",
        ));
    }
    let original = g
        .initial_combat
        .equipment
        .items
        .iter()
        .find(|i| i.origin == origin)
        .ok_or(NativeError::InvalidState)?;
    if original.slot != origin.genesis_slot
        || original.item_id != item.entry
        || original.max_durability != item.maximum_durability
    {
        return Err(NativeError::InvalidState);
    }
    Ok(item)
}

/// FindEquipSlot(NULL_SLOT) is unambiguous only for these source singleton slots.
/// AutoUnequipOffhandIfNeed returns immediately for the source-owned absent offhand.
pub(in crate::native) fn auto_equip_slot(
    g: &QuestProgressionGenesis,
    origin: EquipmentOrigin,
) -> Result<u8, NativeError> {
    let item = source_item(g, origin)?;
    let slot = match item.inventory_type {
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        5 | 20 => 4,
        6 => 5,
        7 => 6,
        8 => 7,
        9 => 8,
        10 => 9,
        16 => 14,
        _ => {
            return Err(NativeError::UnsupportedMechanic(
                "auto-equip singleton armor slot",
            ))
        }
    };
    if slot != origin.genesis_slot
        || g.initial_combat
            .equipment
            .items
            .iter()
            .any(|i| i.slot == 16)
    {
        return Err(NativeError::UnsupportedMechanic(
            "auto-equip original slot or offhand dependency",
        ));
    }
    Ok(slot)
}

fn carried(
    g: &QuestProgressionGenesis,
    id: u64,
    origin: EquipmentOrigin,
) -> Result<inv::Instance, NativeError> {
    let item = source_item(g, origin)?;
    Ok(inv::Instance {
        id,
        item: item.entry,
        count: 1,
        bound: matches!(item.bonding, 1 | 4),
        charges: item.spells.map(|s| s.charges),
        durability: item.maximum_durability,
        maximum_durability: item.maximum_durability,
        kind: inv::InstanceKind::Item,
        duration: 0,
        random_property_id: 0,
        enchantments: [0; 3],
    })
}

pub(in crate::native) fn validate_inventory(
    g: &QuestProgressionGenesis,
    inventory: &inv::State,
    placements: &Placements,
) -> Result<(), NativeError> {
    let l = loot(g)?;
    inventory.validate(&l.inventory)?;
    if inventory.next_item < l.initial_inventory.next_item
        || placements.len() != l.equipment_instances().count()
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    let mut equipped = BTreeMap::<u32, u32>::new();
    let mut positions = BTreeSet::new();
    for (id, origin) in l.equipment_instances() {
        let placement = *placements.get(&id).ok_or(NativeError::InvalidCheckpoint)?;
        let EquipmentPlacement::Slot(slot) = placement else {
            if inventory.slots.values().any(|item| item.id == id) {
                return Err(NativeError::InvalidCheckpoint);
            }
            continue;
        };
        if !positions.insert(slot) {
            return Err(NativeError::InvalidCheckpoint);
        }
        if slot == origin.genesis_slot {
            *equipped.entry(origin.item_id).or_default() += 1;
            if inventory.slots.values().any(|item| item.id == id) {
                return Err(NativeError::InvalidCheckpoint);
            }
        } else if (inv::BACKPACK_START..inv::BACKPACK_END).contains(&slot) {
            if inventory.slots.get(&slot) != Some(&carried(g, id, origin)?) {
                return Err(NativeError::InvalidCheckpoint);
            }
        } else {
            return Err(NativeError::InvalidCheckpoint);
        }
    }
    if equipped != inventory.equipped {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}

fn validate_receipts(
    inventory: &inv::State,
    receipts: &BTreeMap<u64, ItemReceipt>,
) -> Result<(), NativeError> {
    if receipts.len() != inventory.slots.len()
        || inventory.slots.iter().any(|(&slot, item)| {
            receipts
                .get(&item.id)
                .is_none_or(|r| !r.validates(slot, item))
        })
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}

impl EquipmentMoveReceipt {
    /// Recompute the source structural operation. This does not authenticate
    /// custody history; runtime journal validation and projection ancestry do so.
    pub fn validate(&self, g: &QuestProgressionGenesis) -> Result<(), NativeError> {
        // Other originals may already be retired. Infer neither resurrection nor
        // their chronology from absence in the carried inventory.
        self.replay(g, &mut self.before_placements.clone())
    }

    pub fn equips(&self) -> bool {
        self.request.destination == self.origin.genesis_slot
    }

    pub(in crate::native) fn replay(
        &self,
        g: &QuestProgressionGenesis,
        placements: &mut Placements,
    ) -> Result<(), NativeError> {
        let l = loot(g)?;
        if *placements != self.before_placements
            || l.equipment_origin(self.instance) != Some(self.origin)
            || placements.get(&self.instance)
                != Some(&EquipmentPlacement::Slot(self.request.source))
            || self.request.source == self.request.destination
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        source_item(g, self.origin)?;
        if self.context.operation == EquipmentMoveOperation::AutoEquip
            && (!self.equips() || auto_equip_slot(g, self.origin)? != self.request.destination)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        validate_inventory(g, &self.before, placements)?;
        validate_receipts(&self.before, &self.before_receipts)?;
        let after = self.expected_after(g)?;
        if after != self.after || self.after_receipts != self.expected_receipts() {
            return Err(NativeError::InvalidCheckpoint);
        }
        validate_receipts(&after, &self.after_receipts)?;
        let mut next = placements.clone();
        next.insert(
            self.instance,
            EquipmentPlacement::Slot(self.request.destination),
        );
        validate_inventory(g, &after, &next)?;
        *placements = next;
        Ok(())
    }

    pub(in crate::native) fn build(
        g: &QuestProgressionGenesis,
        request: EquipmentMoveRequest,
        instance: u64,
        before: inv::State,
        before_receipts: BTreeMap<u64, ItemReceipt>,
        context: MoveContext,
        placements: &Placements,
    ) -> Result<Self, NativeError> {
        let origin = loot(g)?
            .equipment_origin(instance)
            .ok_or(NativeError::InvalidState)?;
        let mut receipt = Self {
            request,
            instance,
            origin,
            after: before.clone(),
            before,
            before_receipts,
            after_receipts: BTreeMap::new(),
            context,
            before_placements: placements.clone(),
        };
        receipt.after = receipt.expected_after(g)?;
        receipt.after_receipts = receipt.expected_receipts();
        receipt.replay(g, &mut placements.clone())?;
        Ok(receipt)
    }

    fn expected_after(&self, g: &QuestProgressionGenesis) -> Result<inv::State, NativeError> {
        let expected = carried(g, self.instance, self.origin)?;
        let mut after = self.before.clone();
        if self.equips() {
            if !(inv::BACKPACK_START..inv::BACKPACK_END).contains(&self.request.source)
                || after.slots.remove(&self.request.source) != Some(expected)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            *after.equipped.entry(self.origin.item_id).or_default() += 1;
        } else {
            if self.request.source != self.origin.genesis_slot
                || !(inv::BACKPACK_START..inv::BACKPACK_END).contains(&self.request.destination)
                || after.slots.contains_key(&self.request.destination)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            let count = after
                .equipped
                .get_mut(&self.origin.item_id)
                .ok_or(NativeError::InvalidCheckpoint)?;
            *count = count.checked_sub(1).ok_or(NativeError::InvalidCheckpoint)?;
            if *count == 0 {
                after.equipped.remove(&self.origin.item_id);
            }
            after.slots.insert(self.request.destination, expected);
        }
        Ok(after)
    }

    fn reason(&self) -> crate::Digest {
        crate::digest(
            b"benilla-native-armor-move-v2\0",
            &(
                self.request,
                self.instance,
                self.origin,
                &self.before,
                &self.after,
                &self.before_receipts,
                &self.context,
                &self.before_placements,
            ),
        )
    }

    fn expected_receipts(&self) -> BTreeMap<u64, ItemReceipt> {
        let reason = self.reason();
        self.after
            .slots
            .iter()
            .map(|(&slot, item)| {
                (
                    item.id,
                    ItemReceipt::new(
                        self.before_receipts.get(&item.id).map(|r| r.digest),
                        reason,
                        slot,
                        item,
                    ),
                )
            })
            .collect()
    }

    pub(in crate::native) fn replay_work(&self) -> u64 {
        self.context
            .generic_slot
            .as_ref()
            .map_or(0, |s| s.replay_work())
    }
}

/// Structural physical comparison against an independently source-replayed
/// recipe. Callers must not treat persisted inactive flags as that authority.
pub(in crate::native) fn restore_equipment(
    genesis: &CombatGenesis,
    snapshot: EquipmentSnapshot,
    recipe: &init::StatRecipe,
) -> Result<EquipmentState, NativeError> {
    let view = recipe.snapshot();
    if view.items != genesis.recipe.snapshot().items
        || view.inactive_slots.windows(2).any(|s| s[0] >= s[1])
        || view.inactive_slots.iter().any(|slot| {
            !view
                .items
                .iter()
                .any(|item| item.slot == *slot && !item.weapon && *slot < 15)
        })
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    let mut expected = genesis.equipment.clone();
    expected
        .items
        .retain(|item| !view.inactive_slots.contains(&item.slot));
    Ok(EquipmentState::restore(
        snapshot,
        &EquipmentState::from_snapshot(expected)?,
    )?)
}

pub(in crate::native) fn placements(
    g: &QuestProgressionGenesis,
    calls: &[DerivedCall],
) -> Result<Placements, NativeError> {
    let mut result = initial_placements(g);
    for call in calls {
        match call {
            DerivedCall::ItemMembership { movement, .. } => movement.replay(g, &mut result)?,
            DerivedCall::ItemRelocation { movement, .. } => movement.replay(g, &mut result)?,
            DerivedCall::ItemAutoStore { movement, .. } => movement.replay(g, &mut result)?,
            DerivedCall::ItemDestruction { movement, .. } => movement.replay(g, &mut result)?,
            _ => {}
        }
    }
    Ok(result)
}

pub(in crate::native) fn validate_state(
    profile: &Profile,
    state: &State,
) -> Result<(), NativeError> {
    let Some(q) = &state.quest_progression else {
        return Ok(());
    };
    let movements: Vec<_> = q
        .derived_calls
        .iter()
        .filter(|call| {
            matches!(
                call,
                DerivedCall::ItemMembership { .. }
                    | DerivedCall::ItemRelocation { .. }
                    | DerivedCall::ItemAutoStore { .. }
                    | DerivedCall::ItemDestruction { .. }
            )
        })
        .collect();
    if movements.is_empty() {
        return Ok(());
    }
    profile
        .interactions
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic(
            "armor source interaction owner",
        ))?
        .require_armor_movement(profile)?;
    let g = profile
        .quest_progression
        .as_ref()
        .ok_or(NativeError::InvalidState)?;
    let mut previous = None;
    for call in movements {
        let (world, player, digest, sequence, captured_ms) = match call {
            DerivedCall::ItemMembership { movement, .. } => {
                let context = &movement.context;
                if context.in_combat {
                    return Err(NativeError::InvalidCheckpoint);
                }
                if movement.equips() {
                    if context
                        .generic_slot
                        .as_ref()
                        .map(|slot| slot.blocks_pre_unit(profile, state, context.captured_ms))
                        .transpose()?
                        .unwrap_or(false)
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                } else if context.generic_slot.is_some() {
                    return Err(NativeError::InvalidCheckpoint);
                }
                (
                    context.world,
                    context.player,
                    context.profile,
                    context.sequence,
                    context.captured_ms,
                )
            }
            DerivedCall::ItemRelocation { movement, .. } => (
                movement.world,
                movement.player,
                movement.profile,
                movement.sequence,
                movement.captured_ms,
            ),
            DerivedCall::ItemAutoStore { movement, .. } => (
                movement.world,
                movement.player,
                movement.profile,
                movement.sequence,
                movement.captured_ms,
            ),
            DerivedCall::ItemDestruction { movement, .. } => (
                movement.world,
                movement.player,
                movement.profile,
                movement.sequence,
                movement.captured_ms,
            ),
            _ => unreachable!(),
        };
        if digest != profile.digest()
            || world != state.world
            || player != state.player
            || sequence >= state.next_sequence
            || captured_ms < profile.clock_origin_ms
            || captured_ms > state.now_ms
            || !(captured_ms - profile.clock_origin_ms).is_multiple_of(u64::from(profile.tick_ms))
            || previous.is_some_and(|(seq, at)| seq >= sequence || at > captured_ms)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        previous = Some((sequence, captured_ms));
    }
    let custody = placements(g, &q.derived_calls)?;
    validate_inventory(
        g,
        q.inventory.as_ref().ok_or(NativeError::InvalidState)?,
        &custody,
    )?;
    let combat = state.combat.as_ref().ok_or(NativeError::InvalidState)?;
    let expected_inactive: Vec<_> = loot(g)?
        .equipment_instances()
        .filter_map(|(id, origin)| {
            (custody.get(&id) != Some(&EquipmentPlacement::Slot(origin.genesis_slot)))
                .then_some(origin.genesis_slot)
        })
        .collect();
    if expected_inactive != combat.recipe.snapshot().inactive_slots {
        return Err(NativeError::InvalidCheckpoint);
    }
    restore_equipment(
        &g.initial_combat,
        combat.equipment.snapshot(),
        &combat.recipe,
    )?;
    Ok(())
}

/// Carried reducers outside the dedicated route do not own registry relocation/destruction history.
pub(in crate::native) fn reject_registered_slots(
    g: &QuestProgressionGenesis,
    inventory: &inv::State,
    slots: &[u8],
) -> Result<(), NativeError> {
    if let Some(l) = &g.loot {
        if slots
            .iter()
            .filter_map(|slot| inventory.slots.get(slot))
            .any(|item| l.equipment_origin(item.id).is_some())
        {
            return Err(NativeError::UnsupportedMechanic(
                "registered armor consumer",
            ));
        }
    }
    Ok(())
}

/// A carried original changes custody only: no equipment modifier callback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentRelocationReceipt {
    pub movement: inv::MovementReceipt,
    pub instance: u64,
    pub origin: EquipmentOrigin,
    pub before_receipts: BTreeMap<u64, ItemReceipt>,
    pub after_receipts: BTreeMap<u64, ItemReceipt>,
    pub(in crate::native) world: crate::WorldId,
    pub(in crate::native) player: crate::PlayerId,
    pub(in crate::native) profile: crate::Digest,
    pub(in crate::native) sequence: u64,
    pub(in crate::native) captured_ms: u64,
}

impl EquipmentRelocationReceipt {
    pub(in crate::native) fn build(
        g: &QuestProgressionGenesis,
        movement: inv::MovementReceipt,
        before_receipts: BTreeMap<u64, ItemReceipt>,
        state: &State,
        profile: &Profile,
        placements: &Placements,
    ) -> Result<Self, NativeError> {
        let instance = movement
            .before
            .slots
            .get(&movement.source)
            .ok_or(NativeError::InvalidState)?
            .id;
        let origin = loot(g)?
            .equipment_origin(instance)
            .ok_or(NativeError::InvalidState)?;
        let mut result = Self {
            movement,
            instance,
            origin,
            before_receipts,
            after_receipts: BTreeMap::new(),
            world: state.world,
            player: state.player,
            profile: profile.digest(),
            sequence: state.next_sequence,
            captured_ms: state.now_ms,
        };
        result.after_receipts = result.expected_receipts();
        result.replay(g, &mut placements.clone())?;
        Ok(result)
    }

    fn expected_receipts(&self) -> BTreeMap<u64, ItemReceipt> {
        let reason = crate::digest(
            b"benilla-native-armor-relocation-v1\0",
            &(
                &self.movement,
                self.instance,
                self.origin,
                &self.before_receipts,
                self.world,
                self.player,
                self.profile,
                self.sequence,
                self.captured_ms,
            ),
        );
        self.movement
            .after
            .slots
            .iter()
            .map(|(&slot, item)| {
                (
                    item.id,
                    ItemReceipt::new(
                        self.before_receipts.get(&item.id).map(|r| r.digest),
                        reason,
                        slot,
                        item,
                    ),
                )
            })
            .collect()
    }

    pub(in crate::native) fn replay(
        &self,
        g: &QuestProgressionGenesis,
        placements: &mut Placements,
    ) -> Result<(), NativeError> {
        let m = &self.movement;
        if loot(g)?.equipment_origin(self.instance) != Some(self.origin)
            || placements.get(&self.instance) != Some(&EquipmentPlacement::Slot(m.source))
            || m.outcome != inv::MovementOutcome::Moved
            || !m.alive
            || m.source == m.destination
            || m.before.slots.contains_key(&m.destination)
            || m.before.slots.get(&m.source) != Some(&carried(g, self.instance, self.origin)?)
            || m.before.equipped != m.after.equipped
            || m.before.next_item != m.after.next_item
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        m.validate(&loot(g)?.inventory)?;
        validate_inventory(g, &m.before, placements)?;
        validate_receipts(&m.before, &self.before_receipts)?;
        if self.after_receipts != self.expected_receipts() {
            return Err(NativeError::InvalidCheckpoint);
        }
        validate_receipts(&m.after, &self.after_receipts)?;
        let mut next = placements.clone();
        next.insert(self.instance, EquipmentPlacement::Slot(m.destination));
        validate_inventory(g, &m.after, &next)?;
        *placements = next;
        Ok(())
    }
}

/// Source AutoStore changes carried custody without an alive guard or stat callback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentAutoStoreReceipt {
    pub movement: inv::AutoStoreReceipt,
    pub instance: u64,
    pub origin: EquipmentOrigin,
    pub before_receipts: BTreeMap<u64, ItemReceipt>,
    pub after_receipts: BTreeMap<u64, ItemReceipt>,
    pub(in crate::native) world: crate::WorldId,
    pub(in crate::native) player: crate::PlayerId,
    pub(in crate::native) profile: crate::Digest,
    pub(in crate::native) sequence: u64,
    pub(in crate::native) captured_ms: u64,
}

impl EquipmentAutoStoreReceipt {
    pub(in crate::native) fn build(
        g: &QuestProgressionGenesis,
        movement: inv::AutoStoreReceipt,
        before_receipts: BTreeMap<u64, ItemReceipt>,
        state: &State,
        profile: &Profile,
        placements: &Placements,
    ) -> Result<Self, NativeError> {
        let instance = movement
            .before
            .slots
            .get(&movement.source)
            .ok_or(NativeError::InvalidState)?
            .id;
        let origin = loot(g)?
            .equipment_origin(instance)
            .ok_or(NativeError::InvalidState)?;
        let mut result = Self {
            movement,
            instance,
            origin,
            before_receipts,
            after_receipts: BTreeMap::new(),
            world: state.world,
            player: state.player,
            profile: profile.digest(),
            sequence: state.next_sequence,
            captured_ms: state.now_ms,
        };
        result.after_receipts = result.expected_receipts();
        result.replay(g, &mut placements.clone())?;
        Ok(result)
    }

    fn expected_receipts(&self) -> BTreeMap<u64, ItemReceipt> {
        let reason = crate::digest(
            b"benilla-native-armor-autostore-v1\0",
            &(
                &self.movement,
                self.instance,
                self.origin,
                &self.before_receipts,
                self.world,
                self.player,
                self.profile,
                self.sequence,
                self.captured_ms,
            ),
        );
        self.movement
            .after
            .slots
            .iter()
            .map(|(&slot, item)| {
                (
                    item.id,
                    ItemReceipt::new(
                        self.before_receipts.get(&item.id).map(|r| r.digest),
                        reason,
                        slot,
                        item,
                    ),
                )
            })
            .collect()
    }

    pub(in crate::native) fn replay(
        &self,
        g: &QuestProgressionGenesis,
        placements: &mut Placements,
    ) -> Result<(), NativeError> {
        let m = &self.movement;
        if loot(g)?.equipment_origin(self.instance) != Some(self.origin)
            || placements.get(&self.instance) != Some(&EquipmentPlacement::Slot(m.source))
            || m.outcome != inv::AutoStoreOutcome::Stored
            || m.destinations.len() != 1
            || m.destinations[0].count != 1
            || m.source == m.destinations[0].slot
            || m.before.slots.contains_key(&m.destinations[0].slot)
            || m.before.slots.get(&m.source) != Some(&carried(g, self.instance, self.origin)?)
            || m.before.equipped != m.after.equipped
            || m.before.next_item != m.after.next_item
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        m.validate(&loot(g)?.inventory)?;
        validate_inventory(g, &m.before, placements)?;
        validate_receipts(&m.before, &self.before_receipts)?;
        if self.after_receipts != self.expected_receipts() {
            return Err(NativeError::InvalidCheckpoint);
        }
        validate_receipts(&m.after, &self.after_receipts)?;
        let mut next = placements.clone();
        next.insert(
            self.instance,
            EquipmentPlacement::Slot(m.destinations[0].slot),
        );
        validate_inventory(g, &m.after, &next)?;
        *placements = next;
        Ok(())
    }
}

/// Source-selected destruction of one already-carried original. Retirement is
/// permanent custody, not removal of its source registry or a reusable ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentDestroyReceipt {
    pub destruction: inv::DestroyReceipt,
    pub instance: u64,
    pub origin: EquipmentOrigin,
    pub before_receipts: BTreeMap<u64, ItemReceipt>,
    pub after_receipts: BTreeMap<u64, ItemReceipt>,
    pub(in crate::native) before_placements: Placements,
    pub(in crate::native) world: crate::WorldId,
    pub(in crate::native) player: crate::PlayerId,
    pub(in crate::native) profile: crate::Digest,
    pub(in crate::native) sequence: u64,
    pub(in crate::native) captured_ms: u64,
}
impl EquipmentDestroyReceipt {
    pub(in crate::native) fn build(
        g: &QuestProgressionGenesis,
        destruction: inv::DestroyReceipt,
        before_receipts: BTreeMap<u64, ItemReceipt>,
        state: &State,
        profile: &Profile,
        placements: &Placements,
    ) -> Result<Self, NativeError> {
        let instance = destruction
            .before
            .slots
            .get(&destruction.slot)
            .ok_or(NativeError::InvalidState)?
            .id;
        let origin = loot(g)?
            .equipment_origin(instance)
            .ok_or(NativeError::InvalidState)?;
        let mut result = Self {
            destruction,
            instance,
            origin,
            before_receipts,
            after_receipts: BTreeMap::new(),
            before_placements: placements.clone(),
            world: state.world,
            player: state.player,
            profile: profile.digest(),
            sequence: state.next_sequence,
            captured_ms: state.now_ms,
        };
        result.after_receipts = result.expected_receipts();
        result.replay(g, &mut placements.clone())?;
        Ok(result)
    }
    /// Structural reachable receipt validation, not proof of command chronology.
    pub fn validate(&self, g: &QuestProgressionGenesis) -> Result<(), NativeError> {
        self.replay(g, &mut self.before_placements.clone())
    }
    fn expected_receipts(&self) -> BTreeMap<u64, ItemReceipt> {
        let reason = crate::digest(
            b"benilla-native-armor-destruction-v1\0",
            &(
                &self.destruction,
                self.instance,
                self.origin,
                &self.before_receipts,
                &self.before_placements,
                self.world,
                self.player,
                self.profile,
                self.sequence,
                self.captured_ms,
            ),
        );
        self.destruction
            .after
            .slots
            .iter()
            .map(|(&slot, item)| {
                (
                    item.id,
                    ItemReceipt::new(
                        self.before_receipts.get(&item.id).map(|r| r.digest),
                        reason,
                        slot,
                        item,
                    ),
                )
            })
            .collect()
    }
    pub(in crate::native) fn replay(
        &self,
        g: &QuestProgressionGenesis,
        placements: &mut Placements,
    ) -> Result<(), NativeError> {
        let m = &self.destruction;
        let l = loot(g)?;
        if *placements != self.before_placements
            || l.equipment_origin(self.instance) != Some(self.origin)
            || placements.get(&self.instance) != Some(&EquipmentPlacement::Slot(m.slot))
            || !(inv::BACKPACK_START..inv::BACKPACK_END).contains(&m.slot)
            || m.outcome != inv::DestroyOutcome::Whole
            || m.result != inv::InventoryResult::Ok
            || m.removed != 1
            || m.before.slots.get(&m.slot) != Some(&carried(g, self.instance, self.origin)?)
            || m.before.equipped != m.after.equipped
            || m.before.next_item != m.after.next_item
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        // The actual source reducer checks NO_USER_DESTROY and count semantics;
        // there is deliberately no alive/combat/casting predicate here.
        m.validate(&l.inventory)?;
        validate_inventory(g, &m.before, placements)?;
        validate_receipts(&m.before, &self.before_receipts)?;
        let mut expected = m.before.clone();
        expected.slots.remove(&m.slot);
        if expected != m.after || self.after_receipts != self.expected_receipts() {
            return Err(NativeError::InvalidCheckpoint);
        }
        validate_receipts(&m.after, &self.after_receipts)?;
        let mut next = placements.clone();
        next.insert(self.instance, EquipmentPlacement::Retired);
        validate_inventory(g, &m.after, &next)?;
        *placements = next;
        Ok(())
    }
}

#[cfg(test)]
mod operation_tests {
    use super::*;
    fn detached_world() -> NativeWorld {
        let mut world = crate::native::synthetic_equipment_fixture::world();
        let input = crate::native::synthetic_quest_fixture::command(
            &world,
            Command::EquipmentMove(EquipmentMoveRequest {
                source: 4,
                destination: 23,
            }),
        );
        world
            .step(
                1,
                Some(input),
                &mut crate::native::synthetic_equipment_fixture::EquipmentAuthority::default(),
            )
            .unwrap();
        world
    }
    #[test]
    fn selected_original_destroy_retires_handle_for_all_source_count_forms() {
        let world = detached_world();
        let g = world.profile.quest_progression.as_ref().unwrap();
        let q = world.state.quest_progression.as_ref().unwrap();
        let before = q.inventory.as_ref().unwrap();
        let custody = placements(g, &q.derived_calls).unwrap();
        let original = before.slots[&23].id;
        for count in [0, 1, u8::MAX] {
            let destruction = before
                .plan_destroy_backpack(&loot(g).unwrap().inventory, 23, count)
                .unwrap();
            let receipt = EquipmentDestroyReceipt::build(
                g,
                destruction,
                q.inventory_receipts.clone(),
                &world.state,
                &world.profile,
                &custody,
            )
            .unwrap();
            receipt.validate(g).unwrap();
            let mut retired = custody.clone();
            receipt.replay(g, &mut retired).unwrap();
            assert_eq!(retired[&original], EquipmentPlacement::Retired);
            assert_eq!(receipt.destruction.after.next_item, before.next_item);
            assert_eq!(receipt.destruction.after.equipped, before.equipped);
            assert!(!receipt.after_receipts.contains_key(&original));
            assert!(loot(g).unwrap().equipment_origin(original).is_some());
            // A retired original cannot be replay-destroyed or brought back merely
            // by inserting an otherwise-valid pristine instance into its old slot.
            let stable = retired.clone();
            assert!(receipt.replay(g, &mut retired).is_err());
            assert_eq!(retired, stable);
            assert!(validate_inventory(g, before, &retired).is_err());
            assert!(validate_inventory(g, &receipt.destruction.after, &retired).is_ok());
        }
    }
    #[test]
    fn forged_selected_retirement_refuses_without_mutating_custody() {
        let world = detached_world();
        let g = world.profile.quest_progression.as_ref().unwrap();
        let q = world.state.quest_progression.as_ref().unwrap();
        let before = q.inventory.as_ref().unwrap();
        let custody = placements(g, &q.derived_calls).unwrap();
        let destruction = before
            .plan_destroy_backpack(&loot(g).unwrap().inventory, 23, 0)
            .unwrap();
        let valid = EquipmentDestroyReceipt::build(
            g,
            destruction,
            q.inventory_receipts.clone(),
            &world.state,
            &world.profile,
            &custody,
        )
        .unwrap();
        for field in 0..6 {
            let mut forged = valid.clone();
            match field {
                0 => forged.instance += 1,
                1 => forged.destruction.slot = 24,
                2 => forged.destruction.after.next_item += 1,
                3 => {
                    forged.destruction.after.slots = before.slots.clone();
                }
                4 => {
                    forged
                        .before_placements
                        .insert(forged.instance, EquipmentPlacement::Retired);
                }
                5 => {
                    forged.before_receipts.clear();
                }
                _ => unreachable!(),
            }
            forged.after_receipts = forged.expected_receipts();
            let mut unchanged = custody.clone();
            assert!(forged.replay(g, &mut unchanged).is_err());
            assert_eq!(unchanged, custody);
        }
    }
    #[test]
    fn auto_equip_cannot_relabel_unequip_even_with_refreshed_item_checksums() {
        let mut world = crate::native::synthetic_equipment_fixture::world();
        let input = crate::native::synthetic_quest_fixture::command(
            &world,
            Command::EquipmentMove(EquipmentMoveRequest {
                source: 4,
                destination: 23,
            }),
        );
        world
            .step(
                1,
                Some(input),
                &mut crate::native::synthetic_equipment_fixture::EquipmentAuthority::default(),
            )
            .unwrap();
        let mut receipt = world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .find_map(|e| {
                if let Event::EquipmentMoved(r) = e {
                    Some((**r).clone())
                } else {
                    None
                }
            })
            .unwrap();
        receipt.context.operation = EquipmentMoveOperation::AutoEquip;
        receipt.after_receipts = receipt.expected_receipts();
        let g = world.profile().quest_progression.as_ref().unwrap();
        let mut custody = initial_placements(g);
        let before = custody.clone();
        assert!(!receipt.equips());
        assert!(receipt.replay(g, &mut custody).is_err());
        assert_eq!(custody, before);
    }
}
