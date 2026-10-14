// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic Player.cpp / LootMgr.cpp at 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS contributors, GPL-2.0-or-later; Rust GPL-3.0-or-later, no warranty.
//! Native corpse windows and ordinary carried item instances, projected into stock 1.12 UI.
use super::*;
use benilla_sim::{inventory as inv, starter_init as init};

pub(super) struct Metadata {
    items: BTreeMap<u32, p::ItemInfo>,
}
fn valid_text(text: &str) -> bool {
    text.len() <= 16384
        && text
            .chars()
            .all(|c| !c.is_control() || matches!(c, '\n' | '\t'))
}
impl Metadata {
    pub(super) fn load(w: &n::NativeWorld, catalog: &impl ClientCatalog) -> Result<Self, Error> {
        let mut items = BTreeMap::new();
        if let Some(genesis) = w
            .profile()
            .quest_progression
            .as_ref()
            .and_then(|g| g.loot.as_ref())
        {
            for item in genesis.inventory.items() {
                let info = catalog
                    .item_metadata(item.entry)
                    .ok_or(Error::MissingItemMetadata(item.entry))?;
                // Compare every admitted item_template field represented on this wire shape.
                // Server-only duration/extra_flags and absence evidence are bound by genesis.
                if info.class != item.class
                    || info.inventory_type != item.inventory_type
                    || info.display_info_id != item.display_id
                    || info.quality != item.quality
                    || info.flags != item.flags
                    || info.stackable != item.stackable as u32
                    || info.max_count != item.maximum_count as u32
                    || info.bonding != item.bonding
                    || info.max_durability != item.maximum_durability
                    || info.bag_family != item.bag_family
                    || info.container_slots != item.container_slots
                    || info.random_property != item.random_property
                    || info.start_quest != item.start_quest
                    || info.item_set != item.item_set
                    || info.area != item.area
                    || info.map != item.map
                    || info.lock_id != item.lock_id
                    || info.spell_charges_0 != item.spells[0].charges
                    || !info.spells.is_empty()
                    || info.use_spell.is_some()
                    || !valid_text(&info.name)
                    || info.name.is_empty()
                    || !valid_text(&info.description)
                    || !info.dmg_min.is_finite()
                    || !info.dmg_max.is_finite()
                    || !info.ranged_mod_range.is_finite()
                    || info.damages.len() > 5
                    || info.stats.len() > 10
                    || info
                        .damages
                        .iter()
                        .any(|d| !d.min.is_finite() || !d.max.is_finite())
                {
                    return Err(Error::InvalidItemMetadata(item.entry));
                }
                if let Some(starter) = genesis.starter_items.get(&item.entry) {
                    validate_starter(starter, &info)?;
                }
                if !catalog.contains_item_display(item.display_id) {
                    return Err(Error::UnboundItemDisplay(item.entry));
                }
                items.insert(item.entry, info);
            }
        }
        Ok(Self { items })
    }
    pub(super) fn item(&self, item: u32) -> Option<&p::ItemInfo> {
        self.items.get(&item)
    }
}
fn validate_starter(item: &init::ItemRow, info: &p::ItemInfo) -> Result<(), Error> {
    let stats = item
        .stats
        .iter()
        .filter(|s| s.kind != 0 || s.value != 0)
        .map(|s| (s.kind, s.value))
        .collect::<Vec<_>>();
    let damages = item
        .damage
        .iter()
        .filter(|d| f32::from_bits(d.max_bits) > 0.0)
        .map(|d| p::ItemDamage {
            min: f32::from_bits(d.min_bits),
            max: f32::from_bits(d.max_bits),
            school: d.school,
        })
        .collect::<Vec<_>>();
    if info.subclass != item.subclass
        || info.allowable_class != item.allowable_class as i32
        || info.allowable_race != item.allowable_race as i32
        || info.required_level != item.required_level
        || info.required_skill != u32::from(item.required_skill)
        || info.required_skill_rank != item.required_skill_rank
        || info.required_spell != item.required_spell
        || info.required_honor_rank != item.required_honor_rank
        || info.required_rep_faction != item.required_reputation_faction
        || info.stats != stats
        || info.damages != damages
        || info.dmg_min.to_bits() != item.damage[0].min_bits
        || info.dmg_max.to_bits() != item.damage[0].max_bits
        || info.dmg_type != item.damage[0].school
        || info.armor != item.armor
        || info.resistances != item.resistances.map(|r| r as i32)
        || info.block != item.block
        || info.delay_ms != item.delay_ms
    {
        return Err(Error::InvalidItemMetadata(item.id));
    }
    Ok(())
}
fn inventory(state: &n::State) -> Option<&inv::State> {
    state
        .quest_progression
        .as_ref()
        .and_then(|s| s.inventory.as_ref())
}
pub(super) fn guid(w: &n::NativeWorld, instance: u64) -> u64 {
    let hash = crate::hash(
        b"benilla-native-inventory-guid-v1\0",
        &(w.state().world, w.state().player, instance),
    );
    let mut bytes = [0; 8];
    bytes[..6].copy_from_slice(&hash[..6]);
    0x4000_0000_0000_0000 | u64::from_le_bytes(bytes).max(1)
}
pub(super) fn guids(w: &n::NativeWorld) -> impl Iterator<Item = u64> + '_ {
    inventory(w.state())
        .into_iter()
        .flat_map(|i| i.slots.values())
        .map(|i| guid(w, i.id))
}
pub(super) fn player_pairs(w: &n::NativeWorld) -> Vec<(u16, u32)> {
    let Some(state) = inventory(w.state()) else {
        return vec![];
    };
    (inv::FIRST_SLOT..inv::END_SLOT)
        .flat_map(|slot| {
            let guid = state.slots.get(&slot).map_or(0, |i| guid(w, i.id));
            [
                (486 + 2 * u16::from(slot), guid as u32),
                (487 + 2 * u16::from(slot), (guid >> 32) as u32),
            ]
        })
        .collect()
}
fn fields(item: &inv::Instance) -> ObjectFields {
    ObjectFields::from_pairs(&[
        (14, item.count),
        (16, item.charges[0] as u32),
        (17, item.charges[1] as u32),
        (18, item.charges[2] as u32),
        (19, item.charges[3] as u32),
        (20, item.charges[4] as u32),
        (21, u32::from(item.bound)),
        (46, item.durability),
        (47, item.maximum_durability),
    ])
}
pub(super) struct Frame {
    slots: BTreeMap<u8, inv::Instance>,
    pending_push: Option<p::ItemPushResult>,
}
impl Frame {
    pub(super) fn new(state: &n::State) -> Self {
        Self {
            slots: inventory(state).map_or_else(BTreeMap::new, |i| i.slots.clone()),
            pending_push: None,
        }
    }
    fn project(
        &mut self,
        projection: &NativeProjection,
        w: &n::NativeWorld,
        slots: &BTreeMap<u8, inv::Instance>,
    ) -> Vec<SessionEvent> {
        let mut events = vec![];
        let mut pairs = vec![];
        let old = self
            .slots
            .values()
            .map(|item| (item.id, item))
            .collect::<BTreeMap<_, _>>();
        let next = slots.values().map(|item| item.id).collect::<BTreeSet<_>>();
        for id in old.keys().filter(|id| !next.contains(id)) {
            events.push(SessionEvent::ObjectDestroyed(guid(w, *id)));
        }
        for item in slots.values() {
            let guid = guid(w, item.id);
            if let Some(previous) = old.get(&item.id) {
                if *previous != item {
                    events.push(SessionEvent::ObjectValues {
                        guid,
                        fields: fields(item),
                    });
                }
            } else {
                let mut fields = fields(item);
                fields.merge(ObjectFields::from_pairs(&[
                    (0, guid as u32),
                    (1, (guid >> 32) as u32),
                    (2, 3),
                    (3, item.item),
                    (4, 1.0f32.to_bits()),
                    (6, projection.player_guid as u32),
                    (7, (projection.player_guid >> 32) as u32),
                    (8, projection.player_guid as u32),
                    (9, (projection.player_guid >> 32) as u32),
                ]));
                events.push(SessionEvent::ItemCreate {
                    guid,
                    container: false,
                    fields: fields.into_created(p::ObjectType::Item),
                });
            }
        }
        for slot in inv::FIRST_SLOT..inv::END_SLOT {
            let before = self.slots.get(&slot).map(|i| i.id);
            let after = slots.get(&slot).map(|i| i.id);
            if before != after {
                let guid = after.map_or(0, |id| guid(w, id));
                pairs.extend([
                    (486 + 2 * u16::from(slot), guid as u32),
                    (487 + 2 * u16::from(slot), (guid >> 32) as u32),
                ]);
            }
        }
        if !pairs.is_empty() {
            events.push(SessionEvent::ObjectValues {
                guid: projection.player_guid,
                fields: ObjectFields::from_pairs(&pairs),
            });
        }
        self.slots = slots.clone();
        events
    }
    pub(super) fn remove_carried_stack(
        &mut self,
        projection: &NativeProjection,
        w: &n::NativeWorld,
        item: u32,
        count: u32,
    ) -> Result<Vec<SessionEvent>, Error> {
        let mut next = self.slots.clone();
        let (&slot, instance) = next
            .iter_mut()
            .find(|(_, i)| i.item == item)
            .ok_or(Error::StaleProjection)?;
        if count == 0 {
            return Err(Error::StaleProjection);
        }
        instance.count = instance
            .count
            .checked_sub(count)
            .ok_or(Error::StaleProjection)?;
        if instance.count == 0 {
            next.remove(&slot);
        }
        // Player::DestroyItemCount sends destruction/partial-stack updates before reward packets.
        Ok(self.project(projection, w, &next))
    }
    pub(super) fn reward_item(
        &mut self,
        projection: &NativeProjection,
        w: &n::NativeWorld,
        stored: &inv::Stored,
    ) -> Result<Vec<SessionEvent>, Error> {
        self.validate_complete()?;
        if self.slots != stored.before_slots {
            return Err(Error::StaleProjection);
        }
        let mut events = self.project(projection, w, &stored.after_slots);
        events.push(SessionEvent::ItemPushResult(p::ItemPushResult {
            player_guid: projection.player_guid,
            from_npc: true,
            created: false,
            show_in_chat: false,
            bag_slot: 255,
            item_slot: if stored.last_count == stored.count {
                u32::from(stored.last_slot)
            } else {
                u32::MAX
            },
            item_entry: stored.item,
            suffix_factor: 0,
            random_property_id: 0,
            count: stored.count,
        }));
        Ok(events)
    }
    pub(super) fn validate_complete(&self) -> Result<(), Error> {
        if self.pending_push.is_some() {
            Err(Error::StaleProjection)
        } else {
            Ok(())
        }
    }
    pub(super) fn validate_final(&self, w: &n::NativeWorld) -> Result<(), Error> {
        self.validate_complete()?;
        if inventory(w.state()).is_some_and(|state| self.slots != state.slots) {
            return Err(Error::StaleProjection);
        }
        Ok(())
    }
    pub(super) fn finish(
        &mut self,
        projection: &NativeProjection,
        w: &n::NativeWorld,
    ) -> Vec<SessionEvent> {
        inventory(w.state()).map_or_else(Vec::new, |i| self.project(projection, w, &i.slots))
    }
}
impl NativeProjection {
    pub fn item_query(&self, w: &n::NativeWorld, entry: u32) -> Result<SessionEvent, Error> {
        self.check(w)?;
        let mut info = self
            .loot_metadata
            .item(entry)
            .cloned()
            .ok_or(Error::MissingItemMetadata(entry))?;
        info.name = crate::plain_ui_text(&info.name);
        info.description = crate::plain_ui_text(&info.description);
        Ok(SessionEvent::ItemTemplate {
            entry,
            info: Some(Box::new(info)),
        })
    }
    // A fresh stock LootLatch rejects an unsolicited corpse response and sends Release;
    // materialize the saved target so that release remains bound to the durable corpse.
    pub(super) fn initial_loot_window(
        &self,
        w: &n::NativeWorld,
    ) -> Result<Vec<SessionEvent>, Error> {
        let Some(corpse) = w
            .state()
            .quest_progression
            .as_ref()
            .and_then(|s| s.loot_target)
        else {
            return Ok(vec![]);
        };
        let loot = w
            .state()
            .npcs
            .get(&corpse.spawn)
            .filter(|npc| npc.id == corpse)
            .and_then(|npc| npc.corpse_loot.as_ref())
            .filter(|loot| loot.owner == self.player)
            .ok_or(Error::StaleProjection)?;
        self.loot_event(
            w,
            &n::LootEvent::Opened(n::LootWindow {
                corpse,
                items: loot.items().cloned().collect(),
                money: loot.money(),
            }),
            &mut Frame::new(w.state()),
        )
    }
    pub(super) fn initial_inventory(&self, w: &n::NativeWorld) -> Vec<SessionEvent> {
        Frame { slots: BTreeMap::new(), pending_push: None }.finish(self, w)
        .into_iter()
        // initial_player_fields selects the full initial player descriptor; its bag pointers
        // are already emitted by player_values after all item creates.
        .filter(|event| !matches!(event, SessionEvent::ObjectValues { guid, .. } if *guid == self.player_guid))
        .collect()
    }
    pub(super) fn loot_event(
        &self,
        w: &n::NativeWorld,
        event: &n::LootEvent,
        frame: &mut Frame,
    ) -> Result<Vec<SessionEvent>, Error> {
        let mut out = vec![];
        match event {
            n::LootEvent::Opened(window) => out.push(SessionEvent::LootResponse {
                guid: npc_guid(w, window.corpse)?,
                loot_type: p::loot_type::CORPSE,
                gold: window.money,
                items: window
                    .items
                    .iter()
                    .map(|i| p::LootItem {
                        slot: i.slot,
                        item_id: i.item,
                        count: u32::from(i.count),
                        display_info_id: i.display_id,
                        random_property_id: i.random_property as u32,
                        slot_type: p::slot_type::ALLOW_LOOT,
                    })
                    .collect(),
            }),
            // Explicit Release unregisters the player before SendReleaseForAll.
            n::LootEvent::Released {
                corpse,
                send_response: true,
            } => {
                out.push(SessionEvent::LootReleaseResponse {
                    guid: npc_guid(w, *corpse)?,
                    result: 1,
                });
            }
            n::LootEvent::Released {
                send_response: false,
                ..
            } => {}
            n::LootEvent::ItemRemoved { slot, .. } => {
                out.push(SessionEvent::LootRemoved { slot: *slot });
                out.push(SessionEvent::ItemPushResult(
                    frame.pending_push.take().ok_or(Error::StaleProjection)?,
                ));
            }
            n::LootEvent::MoneyRemoved { .. } => out.push(SessionEvent::LootClearMoney),
            n::LootEvent::Stored(stored) => {
                if frame.slots != stored.before_slots {
                    return Err(Error::StaleProjection);
                }
                out.extend(frame.project(self, w, &stored.after_slots));
                // Player::SendNewItem compares the last returned stack's count with this grant.
                // A newly created split stack still uses the sentinel when those counts differ.
                if frame.pending_push.is_some() {
                    return Err(Error::StaleProjection);
                }
                frame.pending_push = Some(p::ItemPushResult {
                    player_guid: self.player_guid,
                    from_npc: false,
                    created: false,
                    show_in_chat: true,
                    bag_slot: 255,
                    item_slot: if stored.last_count == stored.count {
                        u32::from(stored.last_slot)
                    } else {
                        u32::MAX
                    },
                    item_entry: stored.item,
                    suffix_factor: 0,
                    random_property_id: 0,
                    count: stored.count,
                });
            }
            n::LootEvent::StorageFailed { item, result, .. } => {
                out.push(SessionEvent::InventoryFailure {
                    reason: result.protocol(),
                    required_level: None,
                    item_guid: 0,
                    item2_guid: 0, // Loot::SendItem passes itemId as Player::SendEquipError's uint8 slot.
                    bag_slot: *item as u8,
                })
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
#[path = "native_loot_tests.rs"]
mod tests;
