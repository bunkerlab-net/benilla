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

/// Owner-captured source operands establish a reachable context, not an
/// authenticated chronology. Whole TickRecord replay supplies the latter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct MoveContext {
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
}

pub(in crate::native) type Placements = BTreeMap<u64, u8>;

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
        .map(|(id, origin)| (id, origin.genesis_slot))
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
        let slot = *placements.get(&id).ok_or(NativeError::InvalidCheckpoint)?;
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
        let mut custody = initial_placements(g);
        for (&slot, item) in &self.before.slots {
            if loot(g)?.equipment_origin(item.id).is_some() {
                custody.insert(item.id, slot);
            }
        }
        self.replay(g, &mut custody)
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
        if l.equipment_origin(self.instance) != Some(self.origin)
            || placements.get(&self.instance) != Some(&self.request.source)
            || self.request.source == self.request.destination
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        source_item(g, self.origin)?;
        validate_inventory(g, &self.before, placements)?;
        validate_receipts(&self.before, &self.before_receipts)?;
        let after = self.expected_after(g)?;
        if after != self.after || self.after_receipts != self.expected_receipts() {
            return Err(NativeError::InvalidCheckpoint);
        }
        validate_receipts(&after, &self.after_receipts)?;
        let mut next = placements.clone();
        next.insert(self.instance, self.request.destination);
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
            b"benilla-native-armor-move-v1\0",
            &(
                self.request,
                self.instance,
                self.origin,
                &self.before,
                &self.after,
                &self.before_receipts,
                &self.context,
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
        if let DerivedCall::ItemMembership { movement, .. } = call {
            movement.replay(g, &mut result)?;
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
        .filter_map(|call| {
            if let DerivedCall::ItemMembership { movement, .. } = call {
                Some(movement.as_ref())
            } else {
                None
            }
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
    for movement in movements {
        let context = &movement.context;
        if context.profile != profile.digest()
            || context.world != state.world
            || context.player != state.player
            || context.sequence >= state.next_sequence
            || context.captured_ms < profile.clock_origin_ms
            || context.captured_ms > state.now_ms
            || !(context.captured_ms - profile.clock_origin_ms)
                .is_multiple_of(u64::from(profile.tick_ms))
            || previous.is_some_and(|(seq, at)| seq >= context.sequence || at > context.captured_ms)
            || context.in_combat
        {
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
        previous = Some((context.sequence, context.captured_ms));
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
            (custody.get(&id) != Some(&origin.genesis_slot)).then_some(origin.genesis_slot)
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

/// Other carried reducers do not yet own registry relocation/destruction history.
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
