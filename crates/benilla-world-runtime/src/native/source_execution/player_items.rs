// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

// UpdateFields.h:26-54,193-201.
const ITEM_OWNER: usize = 6;
const ITEM_CONTAINED: usize = 8;
const ITEM_COUNT: usize = 14;
const ITEM_DURATION: usize = 15;
const ITEM_CHARGES: usize = 16;
const ITEM_DURABILITY: usize = 46;
const ITEM_MAX_DURABILITY: usize = 47;

// Item.cpp:189-198 and Object.cpp:105-108 initialize the live fields.
#[derive(Debug, Default)]
pub struct SourceItem {
    pub entry: u32,
    pub owner: SourceGuid,
    pub contained: SourceGuid,
    pub count: u32,
    pub duration: u32,
    pub charges: [i32; 5],
    pub flags: u32,
    pub durability: u32,
    pub max_durability: u32,
    pub slot: u8,
    object_type: u32,
    scale: f32,
}

#[derive(Default)]
pub struct SourceItems(BTreeMap<ObjectHandle, SourceItem>);

/// Detached creation borrows the live item registry and the world's item GUID allocator.
pub struct SourcePlayerItemInputs<'a> {
    pub items: &'a mut SourceItems,
    pub guids: &'a mut GlobalGuidAllocators,
    // WorldSession.h:271. Read from the creation session.
    pub logging_out: bool,
}

impl SourceItems {
    pub fn get(&self, handle: ObjectHandle) -> Result<&SourceItem> {
        self.0
            .get(&handle)
            .ok_or_else(|| refuse("execution.item.stale_item_handle"))
    }

    // Item.cpp:958-982,206-229. A zero count or missing template does not construct an Item.
    fn create(
        &mut self,
        objects: &mut SourceObjects,
        guids: &mut GlobalGuidAllocators,
        owner: ObjectHandle,
        entry: u32,
        count: u32,
        templates: &SourceStartupItems,
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
            objects.assign_guid(
                handle,
                allocated.counter,
                SourceGuid::without_entry(HighGuid::Item, allocated.counter),
            )?;
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
            objects.destroy_unbound(handle)?;
        }
        result
    }

    fn destroy(&mut self, objects: &mut SourceObjects, handle: ObjectHandle) -> Result<()> {
        self.get(handle)?;
        objects.destroy_unbound(handle)?;
        self.0.remove(&handle);
        Ok(())
    }
}

struct Outfit<'a> {
    objects: &'a mut SourceObjects,
    unit: &'a mut SourceUnitFields,
    spells: &'a SourceSpellStores,
    templates: &'a SourceStartupItems,
    items: &'a mut SourceItems,
    guids: &'a mut GlobalGuidAllocators,
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
        Err(refuse(
            "execution.player.Player_916.backpack_second_pass_owner",
        ))
    }

    // Player.cpp:952-985.
    fn store_new_item_in_best_slots(
        &mut self,
        live: &mut Outfit<'_>,
        entry: u32,
        count: u32,
    ) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let stop = if self.can_equip_new_item(live, entry)?.is_some() {
            "execution.player.Player_964.equip_new_item_owner"
        } else {
            "execution.player.Player_975.store_new_item_check_owner"
        };
        Err(refuse(stop))
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
        )?
        else {
            return Ok(None);
        };
        let result = self.can_equip_item(live, probe);
        live.items.destroy(live.objects, probe)?;
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
        require(
            proto.max_count <= 0,
            "execution.player.Player_8565.item_max_count_owner",
        )?;
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
}

#[cfg(test)]
#[path = "player_item_tests.rs"]
mod tests;
