// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

// UpdateFields.h:26-54,193-201.
const ITEM_OWNER: usize = 6;
const ITEM_CONTAINED: usize = 8;
const ITEM_FLAGS: usize = 21;
const ITEM_COUNT: usize = 14;
const ITEM_DURATION: usize = 15;
const ITEM_CHARGES: usize = 16;
const ITEM_DURABILITY: usize = 46;
const ITEM_MAX_DURABILITY: usize = 47;
const PLAYER_VISIBLE_ENTRY: usize = 0x104;
const PLAYER_INV_HEAD: usize = 0x1e6;

// Player.h:583-618. All positions here are in INVENTORY_SLOT_BAG_0.
const BACKPACK: std::ops::Range<usize> = 23..39;
const BAG_SLOTS: std::ops::Range<usize> = 19..23;
const KEYRING: std::ops::Range<usize> = 81..97;

// Item.cpp:208 and Player.cpp:9425 observe the actual identity lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemLifetimeObservation {
    Assigned { guid: SourceGuid, entry: u32 },
    Destroyed { guid: SourceGuid },
}

// Item.cpp:189-198 and Object.cpp:105-108 initialize the live fields.
#[derive(Debug, Default)]
pub struct SourceItem {
    pub entry: u32,
    pub owner: SourceGuid,
    pub contained: SourceGuid,
    pub count: u32,
    pub duration: u32,
    pub charges: [i32; 5],
    // Item.cpp:208 calls Object.cpp:105-108. UpdateFields.h:48 and Item.h:147,159.
    enchantments: [[u32; 3]; 7],
    pub flags: u32,
    pub durability: u32,
    pub max_durability: u32,
    pub slot: u8,
    queue_pos: Option<usize>,
    object_type: u32,
    scale: f32,
}

impl SourceItem {
    // Item.h:294 reads the ID word for an enchantment slot.
    pub(super) fn enchantment_id(&self, slot: usize) -> Result<u32> {
        self.enchantments
            .get(slot)
            .map(|words| words[0])
            .ok_or_else(|| refuse("execution.item.Item_294.enchantment_slot_owner"))
    }
}

#[derive(Default)]
pub struct SourceItems(BTreeMap<ObjectHandle, SourceItem>);

/// Detached creation borrows the live item registry and the world's item GUID allocator.
pub struct SourcePlayerItemInputs<'a> {
    pub items: &'a mut SourceItems,
    pub guids: &'a mut GlobalGuidAllocators,
    pub observe: &'a mut dyn FnMut(ItemLifetimeObservation),
    // WorldSession.h:271. Read from the creation session.
    pub logging_out: bool,
}

impl SourceItems {
    pub fn get(&self, handle: ObjectHandle) -> Result<&SourceItem> {
        self.0
            .get(&handle)
            .ok_or_else(|| refuse("execution.item.stale_item_handle"))
    }

    #[allow(clippy::too_many_arguments)]
    // Item.cpp:958-982,206-229. A zero count or missing template does not construct an Item.
    fn create(
        &mut self,
        objects: &mut SourceObjects,
        guids: &mut GlobalGuidAllocators,
        owner: ObjectHandle,
        entry: u32,
        count: u32,
        templates: &SourceStartupItems,
        observe: &mut dyn FnMut(ItemLifetimeObservation),
    ) -> Result<Option<ObjectHandle>> {
        if count == 0 {
            return Ok(None);
        }
        let Some(proto) = templates.item_prototype(entry)? else {
            return Ok(None);
        };
        let count = count.min(proto.stackable);
        require(count != 0, "execution.item.Item_968.stackable_assert")?;
        require(
            proto.inventory_type != 18,
            "execution.item.Bag_77.constructor_owner",
        )?;
        let handle = objects.construct(owner.thread, SourceObjectKind::Item)?;
        let result = (|| {
            let allocated = guids.allocate(HighGuid::Item)?;
            require(
                !allocated.stop_now,
                "execution.item.Item_971.global_guid_stop_owner",
            )?;
            let guid = SourceGuid::without_entry(HighGuid::Item, allocated.counter);
            objects.assign_guid(handle, allocated.counter, guid)?;
            observe(ItemLifetimeObservation::Assigned { guid, entry });
            let mut item = SourceItem::default();
            objects.set_update_value(handle, OBJECT_FIELD_TYPE, &mut item.object_type, 3)?;
            objects.set_update_value(handle, OBJECT_FIELD_ENTRY, &mut item.entry, entry)?;
            objects.set_update_value(handle, OBJECT_FIELD_SCALE_X, &mut item.scale, 1.0)?;
            let owner_guid = objects
                .get(owner)?
                .guid
                .ok_or_else(|| refuse("execution.item.Item_213.owner_guid"))?;
            objects.set_update_guid(handle, ITEM_OWNER, &mut item.owner, owner_guid)?;
            objects.set_update_guid(
                handle,
                ITEM_CONTAINED,
                &mut item.contained,
                SourceGuid::default(),
            )?;
            objects.set_update_value(handle, ITEM_COUNT, &mut item.count, 1)?;
            objects.set_update_value(
                handle,
                ITEM_MAX_DURABILITY,
                &mut item.max_durability,
                proto.max_durability,
            )?;
            objects.set_update_value(
                handle,
                ITEM_DURABILITY,
                &mut item.durability,
                proto.max_durability,
            )?;
            for (idx, &charge) in proto.spell_charges.iter().enumerate() {
                objects.set_update_value(
                    handle,
                    ITEM_CHARGES + idx,
                    &mut item.charges[idx],
                    charge,
                )?;
            }
            objects.set_update_value(handle, ITEM_DURATION, &mut item.duration, proto.duration)?;
            objects.set_update_value(handle, ITEM_COUNT, &mut item.count, count)?;
            require(
                proto.random_property == 0,
                "execution.item.Item_649.random_property_owner",
            )?;
            self.0.insert(handle, item);
            Ok(Some(handle))
        })();
        if result.is_err() {
            self.destroy_handle(objects, handle, observe)?;
        }
        result
    }

    pub(super) fn destroy(
        &mut self,
        objects: &mut SourceObjects,
        handle: ObjectHandle,
        observe: &mut dyn FnMut(ItemLifetimeObservation),
    ) -> Result<()> {
        self.get(handle)?;
        self.destroy_handle(objects, handle, observe)
    }

    fn destroy_handle(
        &mut self,
        objects: &mut SourceObjects,
        handle: ObjectHandle,
        observe: &mut dyn FnMut(ItemLifetimeObservation),
    ) -> Result<()> {
        let guid = objects.get(handle)?.guid;
        objects.destroy_unbound(handle)?;
        self.0.remove(&handle);
        if let Some(guid) = guid {
            observe(ItemLifetimeObservation::Destroyed { guid });
        }
        Ok(())
    }

    fn get_mut(&mut self, handle: ObjectHandle) -> Result<&mut SourceItem> {
        self.0
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.item.stale_item_handle"))
    }
}

struct Outfit<'a> {
    objects: &'a mut SourceObjects,
    unit: &'a mut SourceUnitFields,
    spells: &'a SourceSpellStores,
    templates: &'a SourceStartupItems,
    items: &'a mut SourceItems,
    guids: &'a mut GlobalGuidAllocators,
    observe: &'a mut dyn FnMut(ItemLifetimeObservation),
    class: u8,
    logging_out: bool,
}

impl SourcePlayer {
    // Player.cpp:858-911. LookupEntry skips holes in ascending DBC key order.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_outfit(
        &mut self,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        rows: &PlayerCreateRows,
        spells: &SourceSpellStores,
        templates: &SourceStartupItems,
        class: u8,
        inventory: Option<SourcePlayerItemInputs<'_>>,
    ) -> Result<()> {
        let outfits = rows
            .outfits
            .as_ref()
            .ok_or_else(|| refuse("execution.player.Player_861.outfit_owner"))?;
        let selector = u32::from_le_bytes([self.race, class, self.gender, 0]);
        let row = outfits
            .range(1..)
            .map(|(_, row)| row)
            .find(|row| row.selector == selector)
            .ok_or_else(|| refuse("execution.player.Player_873.outfit_selector"))?;
        let inventory = inventory
            .ok_or_else(|| refuse("execution.player.Player_906.starting_inventory_owner"))?;
        let unit = units.get_mut(objects, self.object)?;
        let mut live = Outfit {
            objects,
            unit,
            spells,
            templates,
            items: inventory.items,
            guids: inventory.guids,
            observe: inventory.observe,
            class,
            logging_out: inventory.logging_out,
        };
        for &entry in &row.items {
            if entry <= 0 {
                continue;
            }
            let entry = entry as u32;
            let Some(proto) = templates.item_prototype(entry)? else {
                continue;
            };
            require(
                (proto.class, proto.subclass) != (0, 5),
                "execution.player.Player_893.food_drink_count_owner",
            )?;
            self.store_new_item_in_best_slots(&mut live, entry, proto.buy_count)?;
        }
        require(
            rows.items(self.race, class).is_empty(),
            "execution.player.Player_911.custom_item_grant_owner",
        )?;
        self.recheck_backpack(&mut live)
    }

    // Player.cpp:952-985.
    fn store_new_item_in_best_slots(
        &mut self,
        live: &mut Outfit<'_>,
        entry: u32,
        count: u32,
    ) -> Result<()> {
        let mut remaining = count;
        while remaining != 0 {
            let Some(slot) = self.can_equip_new_item(live, entry)? else {
                break;
            };
            self.equip_new_item(live, entry, slot)?;
            // Player.cpp:18614-18620. The detached outfit cannot equip an offhand.
            require(
                self.items[16].is_none(),
                "execution.player.Player_18619.offhand_unequip_owner",
            )?;
            remaining -= 1;
        }
        if remaining == 0 {
            return Ok(());
        }
        self.store_new_item(live, entry, remaining)
    }

    // Player.cpp:10054-10078. The fresh quest log is empty at :13446-13450.
    fn equip_new_item(&mut self, live: &mut Outfit<'_>, entry: u32, slot: usize) -> Result<()> {
        let Some(handle) = live.items.create(
            live.objects,
            live.guids,
            self.object,
            entry,
            1,
            live.templates,
            live.observe,
        )?
        else {
            return Ok(());
        };
        let placed = (|| {
            let item = live.items.get_mut(handle)?;
            // Item.cpp:639-681. The plain factory produces no enchantments.
            require(
                item.duration == 0,
                "execution.player.Player_18607.item_duration_owner",
            )?;
            let proto = live
                .templates
                .item_prototype(entry)?
                .ok_or_else(|| refuse("execution.player.Player_10210.item_prototype"))?;
            require(
                !matches!(proto.bonding, 1 | 2 | 4),
                "execution.player.Player_10212.equip_binding_owner",
            )?;
            require(
                self.items.get(slot).is_some_and(Option::is_none),
                "execution.player.Player_10127.equip_merge_owner",
            )?;
            self.place_item(live.objects, handle, slot, item, true)?;
            self.finish_equip(live, proto, slot)
        })();
        if placed.is_err() && self.items.get(slot) != Some(&Some(handle)) {
            live.items.destroy(live.objects, handle, live.observe)?;
        }
        placed
    }

    // Player.cpp:10216-10226. The live item supplies the visible entry.
    fn place_item(
        &mut self,
        objects: &SourceObjects,
        handle: ObjectHandle,
        slot: usize,
        item: &mut SourceItem,
        equip: bool,
    ) -> Result<()> {
        self.set_item_slot(objects, slot, Some(handle))?;
        let owner = objects
            .get(self.object)?
            .guid
            .ok_or_else(|| refuse("execution.player.Player_10218.owner_guid"))?;
        objects.set_update_guid(handle, ITEM_CONTAINED, &mut item.contained, owner)?;
        objects.set_update_guid(handle, ITEM_OWNER, &mut item.owner, owner)?;
        item.slot = slot as u8;
        // Player.cpp:10173-10187. Creator, enchantment and property words remain zero.
        if equip && item.entry != 0 {
            objects.force_update_field(self.object, PLAYER_VISIBLE_ENTRY + 12 * slot)?;
        }
        if !equip {
            require(
                !objects.get(self.object)?.in_world,
                "execution.player.Player_9996.store_in_world_owner",
            )?;
        }
        // Item.cpp:684-739. An item marked CHANGED while NEW stays NEW.
        if item.queue_pos.is_none() {
            item.queue_pos = Some(self.item_update_queue.len());
            self.item_update_queue.push(handle);
        }
        Ok(())
    }

    // Player.cpp:9986-9992,10216-10221,10277-10289. Keep the handle as the only
    // slot value. Update GUID bits only if its derived source GUID changes.
    fn set_item_slot(
        &mut self,
        objects: &SourceObjects,
        slot: usize,
        handle: Option<ObjectHandle>,
    ) -> Result<()> {
        let guid = |handle: Option<ObjectHandle>| -> Result<SourceGuid> {
            match handle {
                None => Ok(SourceGuid::default()),
                Some(handle) => objects
                    .get(handle)?
                    .guid
                    .ok_or_else(|| refuse("execution.player.Player_10217.item_guid_owner")),
            }
        };
        let current = self
            .items
            .get_mut(slot)
            .ok_or_else(|| refuse("execution.player.Player_10216.slot_index"))?;
        let changed = guid(*current)? != guid(handle)?;
        let field = PLAYER_INV_HEAD + 2 * slot;
        objects.write_update_fields(self.object, field..=field + 1, || {
            *current = handle;
            changed
        })?;
        Ok(())
    }

    // Player.cpp:10079-10124. The item is already displayed in its equipment slot.
    fn finish_equip(
        &mut self,
        live: &mut Outfit<'_>,
        proto: &StartupItemPrototype,
        slot: usize,
    ) -> Result<()> {
        let object = live.objects.get(self.object)?;
        let (alive, in_world) = (object.alive, object.in_world);
        if alive {
            require(
                proto.item_set == 0,
                "execution.player.Player_10085.item_set_owner",
            )?;
            self.apply_item_mods(live, proto, slot)?;
            require(
                live.unit.flags & 0x0008_0000 == 0 || proto.class != 2,
                "execution.player.Player_10093.weapon_switch_cooldown_owner",
            )?;
        }
        require(
            !in_world,
            "execution.player.Player_10113.equip_in_world_owner",
        )?;
        // Player.cpp:18235-18259. Only valid on-use spells can add equip cooldowns.
        if proto.flags & 0x80 == 0 {
            require(
                !has_item_spell(live.spells, proto, 0),
                "execution.player.Player_18254.equip_cooldown_owner",
            )?;
        }
        if slot == 15 {
            // StatSystem.cpp:567-574,518-534. The supported route has no hit auras.
            require(
                !live.unit.auras.has_aura_type(54),
                "execution.player.StatSystem_527.weapon_hit_aura_owner",
            )?;
            live.unit.mod_weapon_hit_chance[0] = 0.0;
        }
        Ok(())
    }

    // Player.cpp:7020-7048. Factory durability starts full and the plain item has no enchants.
    fn apply_item_mods(
        &mut self,
        live: &mut Outfit<'_>,
        proto: &StartupItemPrototype,
        slot: usize,
    ) -> Result<()> {
        if slot == 15 {
            // Player.cpp:7176-7188 orders equipment-dependent crit, flat damage, then percent damage.
            for (kind, owner) in [
                (52, "execution.player.Player_7180.weapon_crit_aura_owner"),
                (13, "execution.player.Player_7184.weapon_damage_aura_owner"),
                (
                    79,
                    "execution.player.Player_7188.weapon_damage_pct_aura_owner",
                ),
            ] {
                require(!live.unit.auras.has_aura_type(kind), owner)?;
            }
        }
        self.apply_item_bonuses(live, proto, slot)?;
        // Player.cpp:7255-7281. Only valid on-equip spells reach the cast.
        require(
            !has_item_spell(live.spells, proto, 1),
            "execution.player.Player_7281.equip_spell_owner",
        )
    }

    // Player.cpp:7051-7173. Creation accumulates armor and writes staff damage/attack time.
    fn apply_item_bonuses(
        &mut self,
        live: &mut Outfit<'_>,
        proto: &StartupItemPrototype,
        slot: usize,
    ) -> Result<()> {
        require(
            proto.stat_values.iter().all(|&value| value == 0),
            "execution.player.Player_7063.item_stat_owner",
        )?;
        let can_modify = live.unit.loaded_creature.can_modify_stats;
        if proto.armor != 0 {
            live.unit.resistances.apply_player_item_armor(
                live.objects,
                self.object,
                proto.armor as f32,
                can_modify,
            )?;
        }
        require(
            proto.block == 0,
            "execution.player.Player_7098.item_block_owner",
        )?;
        for (value, owner) in proto.resistances.iter().zip([
            "execution.player.Player_7101.item_holy_resistance_owner",
            "execution.player.Player_7104.item_fire_resistance_owner",
            "execution.player.Player_7107.item_nature_resistance_owner",
            "execution.player.Player_7110.item_frost_resistance_owner",
            "execution.player.Player_7113.item_shadow_resistance_owner",
            "execution.player.Player_7116.item_arcane_resistance_owner",
        ]) {
            require(*value == 0, owner)?;
        }
        if proto.class == 2 {
            require(
                slot == 15,
                "execution.player.Player_7122.weapon_attack_type_owner",
            )?;
            let [min, max, school] = proto.first_damage;
            if f32::from_bits(max) != 0.0 {
                live.unit
                    .creature_damage
                    .set_mainhand_base_weapon([f32::from_bits(min), f32::from_bits(max)]);
                require(
                    school == 0,
                    "execution.player.Player_7152.weapon_damage_school_owner",
                )?;
                require(
                    f32::from_bits(proto.second_damage_max) == 0.0,
                    "execution.player.Player_7147.extra_weapon_damage_line_owner",
                )?;
            }
            // Unit.h:1191-1204,1354; Unit.cpp:10889-10904. Construction has normal form and speed.
            require(
                live.unit.shapeshift_form == 0,
                "execution.player.Unit_1193.weapon_shapeshift_owner",
            )?;
            if live.unit.flags & 0x0020_0000 == 0 && proto.delay != 0 {
                live.objects.set_update_value(
                    self.object,
                    UNIT_FIELD_BASEATTACKTIME,
                    &mut self.attack_times[0],
                    proto.delay as f32,
                )?;
                require(
                    !can_modify,
                    "execution.player.Player_7172.weapon_damage_update_owner",
                )?;
            }
        }
        Ok(())
    }
    // Player.cpp:9418-9429. Destroy the probe before returning the equip result.
    fn can_equip_new_item(&self, live: &mut Outfit<'_>, entry: u32) -> Result<Option<usize>> {
        let Some(probe) = live.items.create(
            live.objects,
            live.guids,
            self.object,
            entry,
            1,
            live.templates,
            live.observe,
        )?
        else {
            return Ok(None);
        };
        let result = self.can_equip_item(live, probe);
        live.items.destroy(live.objects, probe, live.observe)?;
        result
    }

    // Player.cpp:9432-9550. No swap, auto slot, direct action.
    fn can_equip_item(&self, live: &Outfit<'_>, handle: ObjectHandle) -> Result<Option<usize>> {
        let item = live.items.get(handle)?;
        let proto = live
            .templates
            .item_prototype(item.entry)?
            .ok_or_else(|| refuse("execution.player.Player_9438.item_prototype"))?;
        let owner = live.objects.get(self.object)?;
        // Item.cpp:997-1011 returns early for the owner. The Item constructor has no loot.
        require(
            owner.guid == Some(item.owner) || item.flags & 1 == 0,
            "execution.player.Player_9446.item_owner",
        )?;
        self.can_take_more_similar_items(live, proto, item.entry, item.count, Some(handle))?;
        require(
            live.unit.flags & 0x0004_0000 == 0,
            "execution.player.Player_9459.stunned",
        )?;
        // ItemPrototype.h:505-522. Shield, holdable, relic, weapon and projectile can change.
        if !matches!(proto.inventory_type, 14 | 23 | 28) && !matches!(proto.class, 2 | 6) {
            require(
                live.unit.flags & 0x0008_0000 == 0,
                "execution.player.Player_9466.in_combat",
            )?;
        }
        require(
            !live.logging_out,
            "execution.player.Player_9471.logging_out",
        )?;
        require(
            !live.unit.spell_slots.is_non_melee_spell_casted(
                &live.unit.spell_starts,
                live.spells,
                false,
                false,
                false,
                false,
                false,
            )?,
            "execution.player.Player_9474.non_melee_cast",
        )?;
        require(
            !live.unit.auras.has_aura_type(176),
            "execution.player.Player_9478.spirit_of_redemption",
        )?;

        self.can_use_item_template(live, proto)?;
        // Player.cpp:7858-7905,7927-8101. Non-equipment returns NULL_SLOT.
        let slot = match proto.inventory_type {
            0 => return Ok(None),
            4 => 3,
            20 => 4,
            7 => 6,
            8 => 7,
            17 => {
                require(
                    !live.unit.loaded_creature.can_dual_wield,
                    "execution.player.Player_8016.dual_wield_slot_owner",
                )?;
                15
            }
            _ => return Err(refuse("execution.player.Player_7932.equip_slot_type_owner")),
        };
        require(
            self.items[slot].is_none(),
            "execution.player.Player_7905.occupied_equipment_slot",
        )?;
        require(owner.alive, "execution.player.Player_9786.dead")?;
        // Player.cpp:9797 checks the template again. The same detached operands already passed.
        let skill = match (proto.class, proto.subclass) {
            (2, 10) => 136,
            (4, 1) => 415,
            (4, 0) => 0,
            (2, _) => return Err(refuse("execution.item.Item_584.weapon_skill_owner")),
            (4, _) => return Err(refuse("execution.item.Item_591.armor_skill_owner")),
            _ => 0,
        };
        require(
            skill == 0 || self.skill_value(skill) != 0,
            "execution.player.Player_9807.item_proficiency",
        )?;
        require(
            proto.required_reputation_faction == 0,
            "execution.player.Player_9810.item_reputation_owner",
        )?;
        require(
            proto.flags & 0x0008_0000 == 0,
            "execution.player.Player_19593.unique_equip_owner",
        )?;
        require(
            proto.class != 11,
            "execution.player.Player_9498.quiver_owner",
        )?;
        if proto.inventory_type == 17 {
            require(
                self.items[16].is_none(),
                "execution.player.Player_9545.two_hand_offhand_owner",
            )?;
        }
        Ok(Some(slot))
    }

    // Player.cpp:9819-9847.
    fn can_use_item_template(&self, live: &Outfit<'_>, proto: &StartupItemPrototype) -> Result<()> {
        require(
            proto.allowable_class & (1 << (live.class - 1)) != 0
                && proto.allowable_race & (1 << (self.race - 1)) != 0,
            "execution.player.Player_9826.item_race_class",
        )?;
        require(
            proto.required_skill == 0,
            "execution.player.Player_9830.required_skill_owner",
        )?;
        require(
            proto.required_spell == 0 || self.has_spell(proto.required_spell),
            "execution.player.Player_9837.required_spell",
        )?;
        require(
            proto.required_honor_rank as u8 == 0,
            "execution.player.Player_9839.honor_rank_owner",
        )?;
        let level = live
            .unit
            .power_fields
            .as_ref()
            .ok_or_else(|| refuse("execution.player.Player_9842.level_owner"))?
            .level;
        require(
            level >= proto.required_level,
            "execution.player.Player_9843.item_level",
        )
    }

    // Player.cpp:8552-8577,8170-8208. Named refusals cover other containers.
    fn can_take_more_similar_items(
        &self,
        live: &Outfit<'_>,
        proto: &StartupItemPrototype,
        entry: u32,
        count: u32,
        source: Option<ObjectHandle>,
    ) -> Result<()> {
        if proto.max_count <= 0 {
            return Ok(());
        }
        let count_entry = |i, total: &mut u32| -> Result<()> {
            if let Some(handle) = self.items[i] {
                if Some(handle) != source {
                    let item = live.items.get(handle)?;
                    if item.entry == entry {
                        *total = total.wrapping_add(item.count);
                    }
                }
            }
            Ok(())
        };
        let mut total = 0;
        for slot in (0..39).chain(KEYRING) {
            count_entry(slot, &mut total)?;
        }
        require(
            self.items[BAG_SLOTS].iter().all(Option::is_none),
            "execution.player.Player_8189.bag_item_count_owner",
        )?;
        for slot in 39..63 {
            count_entry(slot, &mut total)?;
        }
        require(
            self.items[63..69].iter().all(Option::is_none),
            "execution.player.Player_8204.bank_bag_item_count_owner",
        )?;
        require(
            total.wrapping_add(count) <= proto.max_count as u32,
            "execution.player.Player_8571.item_max_count_exceeded",
        )
    }

    // Player.cpp:8822-9194. Direct bag-zero and auto-bag, no specific slot or swap.
    fn can_store_item(
        &self,
        live: &Outfit<'_>,
        source: Option<ObjectHandle>,
        entry: u32,
        count: u32,
        proto: &StartupItemPrototype,
    ) -> Result<usize> {
        if let Some(handle) = source {
            let item = live.items.get(handle)?;
            require(
                live.objects.get(self.object)?.guid == Some(item.owner) || item.flags & 1 == 0,
                "execution.player.Player_8848.item_owner",
            )?;
        }
        self.can_take_more_similar_items(live, proto, entry, count, source)?;
        if proto.stackable > 1 {
            for handle in KEYRING.chain(BACKPACK).filter_map(|i| self.items[i]) {
                if Some(handle) == source {
                    continue;
                }
                let present = live.items.get(handle)?;
                require(
                    present.entry != entry || present.count >= proto.stackable,
                    "execution.player.Player_8798.stack_merge_owner",
                )?;
            }
            if source.is_some() {
                require(
                    self.items[BAG_SLOTS].iter().all(Option::is_none),
                    "execution.player.Player_9092.merge_into_bag_owner",
                )?;
            }
        }
        if source.is_some() {
            require(
                proto.bag_family == 0,
                "execution.player.Player_9109.special_bag_owner",
            )?;
        } else {
            require(
                proto.bag_family != 9,
                "execution.player.Player_8964.keyring_owner",
            )?;
        }
        let mut slots = BACKPACK;
        let slot = slots
            .find(|&i| self.items[i].is_none() || self.items[i] == source)
            .ok_or_else(|| refuse("execution.player.Player_9194.backpack_full"))?;
        require(
            count <= proto.stackable,
            "execution.player.Player_9949.split_stack_owner",
        )?;
        Ok(slot)
    }

    // Player.cpp:9914-9927. The fresh quest log has no occupied slots (:13446-13450).
    fn store_new_item(&mut self, live: &mut Outfit<'_>, entry: u32, count: u32) -> Result<()> {
        let proto = live
            .templates
            .item_prototype(entry)?
            .ok_or_else(|| refuse("execution.player.Player_8827.store_template_owner"))?;
        let slot = self.can_store_item(live, None, entry, count, proto)?;
        require(
            proto.random_property == 0,
            "execution.item.Item_649.random_property_owner",
        )?;
        if let Some(handle) = live.items.create(
            live.objects,
            live.guids,
            self.object,
            entry,
            count,
            live.templates,
            live.observe,
        )? {
            let result = self.store_item(live, handle, slot, count, proto);
            if result.is_err() && self.items[slot] != Some(handle) {
                live.items.destroy(live.objects, handle, live.observe)?;
            }
            result?;
        }
        Ok(())
    }

    // Player.cpp:9929-10015. A single empty backpack destination reuses the original item.
    fn store_item(
        &mut self,
        live: &mut Outfit<'_>,
        handle: ObjectHandle,
        slot: usize,
        count: u32,
        proto: &StartupItemPrototype,
    ) -> Result<()> {
        let item = live.items.get_mut(handle)?;
        live.objects
            .set_update_value(handle, ITEM_COUNT, &mut item.count, count)?;
        if matches!(proto.bonding, 1 | 4) {
            live.objects
                .update_flags(handle, ITEM_FLAGS, &mut item.flags, 1, true)?;
        }
        self.place_item(live.objects, handle, slot, item, false)?;
        require(
            item.duration == 0,
            "execution.player.Player_18607.item_duration_owner",
        )
    }

    // Player.cpp:916-949. This pass checks the existing Item rather than creating another probe.
    fn recheck_backpack(&mut self, live: &mut Outfit<'_>) -> Result<()> {
        for slot in BACKPACK {
            let Some(handle) = self.items[slot] else {
                continue;
            };
            require(
                self.can_equip_item(live, handle)?.is_none(),
                "execution.player.Player_925.backpack_equip_owner",
            )?;
            let item = live.items.get(handle)?;
            let (entry, count) = (item.entry, item.count);
            let proto = live
                .templates
                .item_prototype(entry)?
                .ok_or_else(|| refuse("execution.player.Player_8827.store_template_owner"))?;
            let destination = self.can_store_item(live, Some(handle), entry, count, proto)?;
            self.remove_backpack_item(live, handle, slot)?;
            self.store_item(live, handle, destination, count, proto)?;
            // Player.cpp:9850-9874. A live non-ammo item returns ONLY_AMMO_CAN_GO_HERE.
            if live.objects.get(self.object)?.alive {
                require(
                    proto.inventory_type != 24,
                    "execution.player.Player_9863.ammo_use_owner",
                )?;
            }
        }
        Ok(())
    }

    // Player.cpp:10229-10294. Remove only the placement; preserve the owner and queue position.
    fn remove_backpack_item(
        &mut self,
        live: &mut Outfit<'_>,
        handle: ObjectHandle,
        slot: usize,
    ) -> Result<()> {
        let item = live.items.get_mut(handle)?;
        require(
            item.duration == 0,
            "execution.player.Player_18599.remove_item_duration_owner",
        )?;
        self.set_item_slot(live.objects, slot, None)?;
        live.objects.set_update_guid(
            handle,
            ITEM_CONTAINED,
            &mut item.contained,
            SourceGuid::default(),
        )?;
        item.slot = u8::MAX;
        require(
            !live.objects.get(self.object)?.in_world,
            "execution.player.Player_10293.remove_in_world_owner",
        )
    }
}

fn has_item_spell(spells: &SourceSpellStores, proto: &StartupItemPrototype, trigger: u32) -> bool {
    proto
        .spell_ids
        .iter()
        .zip(&proto.spell_triggers)
        .any(|(&id, &kind)| id != 0 && kind == trigger && spells.template(id).is_some())
}

#[cfg(test)]
#[path = "player_item_tests.rs"]
mod tests;
