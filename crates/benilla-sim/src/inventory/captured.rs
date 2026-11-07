// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, ObjectMgr.cpp:2557-2815,
// Player.cpp:857-985,8822-9195,9432-9555; upstream GPL-2.0-or-later, CMaNGOS contributors.
//! Checked captured-item projection, not native-world admission or new equipment authority.
use super::*;
use crate::starter_init::{ItemRow, StartingItemRow};

pub const ADAPTER: &str = "cmangos-8ec338a1-captured-item-admission-v1";

/// Two projections of the same captured prototype. Shared fields must agree exactly.
/// The producer retains the complete source row and its hash outside these runtime inputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedItem {
    pub inventory: Item,
    pub placement: ItemRow,
    pub buy_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub item: u32,
    pub field: &'static str,
    pub reason: &'static str,
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "captured item {} field {}: {}",
            self.item, self.field, self.reason
        )
    }
}
impl std::error::Error for Refusal {}

impl CapturedItem {
    fn refuse(&self, field: &'static str, reason: &'static str) -> Refusal {
        Refusal {
            item: self.inventory.entry,
            field,
            reason,
        }
    }

    fn validate(&self) -> Result<(), Refusal> {
        let i = &self.inventory;
        let p = &self.placement;
        // ItemPrototype.h:363-380. Refuse loader repairs instead of silently changing source rows.
        const SUBCLASSES: [u32; 16] = [9, 8, 21, 1, 10, 1, 5, 14, 1, 11, 1, 4, 1, 2, 1, 6];
        if i.entry == 0 || i.class >= 16 || p.subclass >= SUBCLASSES[i.class as usize] {
            return Err(self.refuse("class/subclass", "prototype_loader_repair_required"));
        }
        if i.inventory_type >= 29 || i.quality > 6 || i.display_id == 0 {
            return Err(self.refuse("InventoryType/Quality/displayid", "prototype_domain"));
        }
        if !(1..=255).contains(&i.stackable) || i.maximum_count < 0 || i.bonding > 4 {
            return Err(self.refuse("stackable/maxcount/bonding", "prototype_domain"));
        }
        if p.required_skill >= 763 {
            return Err(self.refuse("RequiredSkill", "prototype_loader_repair_required"));
        }
        if i.entry != p.id
            || i.class != p.class
            || i.inventory_type != p.inventory_type
            || i.flags != p.flags
            || i.stackable as u32 != p.stackable
            || i.maximum_count as u32 != p.unique_max_count
            || i.maximum_durability != p.max_durability
            || i.bag_family != p.bag_family
            || i.container_slots != p.container_slots
            || i.random_property != p.random_property
            || i.duration != p.duration
            || i.item_set != p.item_set
            || (i.script_id != 0) != p.scripted
            || i.spells
                .iter()
                .zip(p.spells)
                .any(|(a, b)| a.id != b.id || a.trigger != b.trigger)
        {
            return Err(self.refuse("projection", "inventory_placement_mismatch"));
        }
        if i.spells
            .iter()
            .any(|s| s.trigger >= 6 || s.ppm_rate_bits & 0x7f80_0000 == 0x7f80_0000)
        {
            return Err(self.refuse("spells", "prototype_spell_domain"));
        }
        if p.stats.iter().any(|s| s.value != 0 && s.kind >= 8)
            || p.damage.iter().any(|d| {
                d.school >= 7
                    || [d.min_bits, d.max_bits]
                        .into_iter()
                        .any(|b| b & 0x7f80_0000 == 0x7f80_0000)
            })
        {
            return Err(self.refuse("stats/damage", "prototype_loader_repair_required"));
        }
        Ok(())
    }

    fn validate_carried(&self) -> Result<(), Refusal> {
        self.validate()?;
        let i = &self.inventory;
        for (field, present) in [
            ("startquest", i.start_quest != 0),
            ("RandomProperty", i.random_property != 0),
            ("ScriptName", i.script_id != 0),
            ("Duration", i.duration != 0),
            ("area", i.area != 0),
            ("Map", i.map != 0),
            ("BagFamily", i.bag_family != 0),
            ("itemset", i.item_set != 0),
            ("lockid", i.lock_id != 0),
            ("ExtraFlags", i.extra_flags != 0),
            // PLAYERCAST affects use, not carried storage (ItemPrototype.h:75).
            ("Flags", i.flags & !(0x800 | 0x40) != 0),
        ] {
            if present {
                return Err(self.refuse(field, "carried_dependency_unsupported"));
            }
        }
        let bag = i.class == 1
            && i.inventory_type == 18
            && (1..=36).contains(&i.container_slots)
            && i.stackable == 1;
        if i.class == 11
            || i.inventory_type == 27
            || (!bag && (i.class == 1 || i.inventory_type == 18 || i.container_slots != 0))
        {
            return Err(self.refuse(
                "ContainerSlots/InventoryType",
                "carried_container_unsupported",
            ));
        }
        if i.spells.iter().any(|s| s.id != 0 && s.trigger != 0) {
            return Err(self.refuse("spells", "carried_equip_proc_spell_unsupported"));
        }
        Ok(())
    }

    /// Compile a corpse-carried template. This does not establish the independent inventory audit.
    pub fn compile_carried(self) -> Result<Item, Refusal> {
        self.validate_carried()?;
        Ok(self.inventory)
    }

    /// Project into the existing initializer. Actual learned skills, masks, level,
    /// capacity and placement remain the initializer's responsibility.
    pub fn compile_starting(self) -> Result<StartingItemRow, Refusal> {
        self.validate_carried()?;
        if self.buy_count == 0 || self.buy_count > 10_000 {
            return Err(self.refuse("BuyCount", "outfit_quantity_domain"));
        }
        let p = &self.placement;
        let kind = p.inventory_type;
        let supported = match p.class {
            0 => p.subclass <= 8 && kind == 0,
            2 => {
                matches!(p.subclass, 0 | 1 | 4 | 5 | 6 | 7 | 8 | 10 | 13 | 15 | 20)
                    && matches!(kind, 0 | 13 | 17 | 21)
            }
            4 => p.subclass <= 4 && matches!(kind, 0..=12 | 16 | 19 | 20),
            15 => p.subclass == 0 && kind == 0,
            _ => false,
        };
        if !supported {
            return Err(self.refuse(
                "class/subclass/InventoryType",
                "starting_placement_unsupported",
            ));
        }
        if p.flags & !0x40 != 0
            || p.block != 0
            || p.required_honor_rank != 0
            || p.required_reputation_faction != 0
        {
            return Err(self.refuse(
                "Flags/block/requirements",
                "starting_dependency_unsupported",
            ));
        }
        // Use the same carried lifecycle closure, but never convert storage into equip authority.
        let first_spell_category = self.inventory.spells[0].category;
        Ok(StartingItemRow {
            item: self.placement,
            buy_count: self.buy_count,
            first_spell_category,
        })
    }
}
/// Checked carried prototypes without host absence evidence or mutation authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedItems {
    rows: BTreeMap<u32, Item>,
}
impl CheckedItems {
    pub fn item(&self, entry: u32) -> Result<&Item, Error> {
        self.rows.get(&entry).ok_or(Error::Invalid("unknown_item"))
    }
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.rows.values()
    }
    /// Only the independent host audit can turn checked prototypes into a storage catalog.
    pub fn into_catalog(self, audit: Audit) -> Result<Catalog, Error> {
        let catalog = Catalog {
            rows: self.rows,
            audit,
        };
        catalog.validate()?;
        catalog.storage_dependencies_ready()?;
        Ok(catalog)
    }
}

pub fn compile_items(rows: Vec<CapturedItem>) -> Result<CheckedItems, Vec<Refusal>> {
    if rows.len() > 2048 {
        return Err(vec![Refusal {
            item: 0,
            field: "catalog",
            reason: "item_row_limit",
        }]);
    }
    let mut items = BTreeMap::new();
    let mut refusals = Vec::new();
    for row in rows {
        match items.entry(row.inventory.entry) {
            std::collections::btree_map::Entry::Occupied(_) => {
                refusals.push(row.refuse("entry", "duplicate_item"));
            }
            std::collections::btree_map::Entry::Vacant(entry) => match row.compile_carried() {
                Ok(item) => {
                    entry.insert(item);
                }
                Err(refusal) => refusals.push(refusal),
            },
        }
    }
    if refusals.is_empty() {
        Ok(CheckedItems { rows: items })
    } else {
        Err(refusals)
    }
}

/// All rows must pass. No successful partial catalog or inferred audit is returned.
pub fn compile_catalog(rows: Vec<CapturedItem>, audit: Audit) -> Result<Catalog, Vec<Refusal>> {
    let items = compile_items(rows)?;
    let catalog = Catalog {
        rows: items.rows,
        audit,
    };
    catalog.validate().map_err(|_| {
        vec![Refusal {
            item: 0,
            field: "catalog",
            reason: "invalid_catalog",
        }]
    })?;
    catalog.storage_dependencies_ready().map_err(|_| {
        vec![Refusal {
            item: 0,
            field: "audit",
            reason: "inventory_dependency_not_absent",
        }]
    })?;
    Ok(catalog)
}
