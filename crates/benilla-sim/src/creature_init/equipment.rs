// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Creature.cpp:388-429/1771-1790/2747-2779; ObjectMgr.cpp:539-545/1561-1600/
// 2580-2608/2711-2718/2763-2767; Unit.h:1191-1206. Upstream GPL-2.0-or-later;
// Rust translation GPL-3.0-or-later. See ../../CREATURE-INIT.md.
use super::*;

/// Raw creature_equip_template columns. Entry zero is retained but never selected
/// by LoadEquipment(0); all three slots remain part of the raw source identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct EquipmentTemplateRow {
    pub entry: u32,
    pub item_entries: [u32; 3],
}
/// Raw item damage, retained but never substituted for creature damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ItemDamage {
    pub minimum: SpellFloat,
    pub maximum: SpellFloat,
    pub school: u32,
}
/// Relevant raw item_template metadata before loader repairs. The capture owner
/// must retain/dispose remaining item columns; this is not an item-use profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ItemTemplateRow {
    pub entry: u32,
    pub display_id: u32,
    pub class: u32,
    pub subclass: u32,
    /// SQL signed tinyint; GetUInt32 then SetByteValue preserves the low byte.
    pub material: i32,
    pub inventory_type: u32,
    pub sheath: u32,
    /// Item Delay is retained, not used as the creature attack interval.
    pub delay_ms: u32,
    pub damage: [ItemDamage; 5],
}
/// Fresh SetVirtualItem arithmetic, not rendering provenance or admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialVirtualItem {
    pub item_entry: u32,
    pub display_id: u32,
    pub info: [u32; 2],
    pub has_weapon: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialEquipment {
    pub equipment_template_id: u32,
    pub mainhand: Option<InitialVirtualItem>,
}
impl InitialEquipment {
    /// Derived only from validated initial metadata and effective raw UnitFlags.
    /// Dynamic disarm aura application/removal and later equipment are unsupported.
    pub(super) fn mainhand_weapon_unusable(self, effective_unit_flags: u32) -> bool {
        self.mainhand.is_some_and(|item| item.info[0] & 0xff == 2)
            && effective_unit_flags & 0x0020_0000 != 0
    }
}
const MAX_ITEM_SUBCLASSES: [u32; 16] = [9, 8, 21, 1, 10, 1, 5, 14, 1, 11, 1, 4, 1, 2, 1, 6];

pub(super) fn prepare(input: &Input, spawn: SpawnTemplate) -> Result<InitialEquipment> {
    // Missing is never inferred to mean empty, even on a no-equipment path.
    let equipment_rows = input
        .equipment_templates
        .require("creature_equip_template", input.template.entry)?;
    let item_rows = input
        .item_templates
        .require("item_template equipment metadata", input.template.entry)?;
    let mut items = BTreeMap::new();
    for row in item_rows {
        if items.insert(row.entry, row).is_some() {
            return Err(InitError::Invalid("duplicate equipment item row"));
        }
        let Some(max_subclass) = MAX_ITEM_SUBCLASSES.get(row.class as usize) else {
            return Err(InitError::Unsupported("item class loader repair"));
        };
        if row.subclass >= *max_subclass {
            return Err(InitError::Unsupported("item subclass loader repair"));
        }
        if row.inventory_type >= 29 {
            return Err(InitError::Unsupported("item inventory-type loader repair"));
        }
        if row.sheath >= 8 {
            return Err(InitError::Unsupported("item sheath loader repair"));
        }
        if !(-128..=127).contains(&row.material) {
            return Err(InitError::Invalid("item material signed-tinyint domain"));
        }
        if row.damage.iter().any(|line| line.school >= 7) {
            return Err(InitError::Unsupported("item damage-school loader repair"));
        }
    }
    let mut equipment = BTreeMap::new();
    for row in equipment_rows {
        if equipment.insert(row.entry, row).is_some() {
            return Err(InitError::Invalid("duplicate equipment template row"));
        }
        // Native validation checks every supplied equipment row and all slots,
        // without inventing per-slot or class/inventory compatibility rules.
        for &entry in row.item_entries.iter().filter(|&&entry| entry != 0) {
            let item = items.get(&entry).ok_or(InitError::Missing {
                table: "item_template equipment metadata",
                id: entry,
            })?;
            if !matches!(
                item.inventory_type,
                13 | 14 | 15 | 17 | 21 | 22 | 23 | 25 | 26 | 28
            ) {
                return Err(InitError::Unsupported(
                    "equipment inventory-type loader repair",
                ));
            }
        }
    }
    // The native template loader validates this reference before spawn overrides.
    let template_id = input.template.equipment_template_id;
    if template_id != 0 && !equipment.contains_key(&template_id) {
        return Err(InitError::Missing {
            table: "creature_equip_template template reference",
            id: template_id,
        });
    }
    let id = if spawn.equipment_id == -1 {
        template_id
    } else {
        spawn.equipment_id as u32
    };
    if id == 0 {
        // Explicit spawn zero skips loading; template zero force-clears. These
        // coincide only for the fresh constructor, never for existing equipment.
        return Ok(InitialEquipment {
            equipment_template_id: 0,
            mainhand: None,
        });
    }
    let row = equipment.get(&id).ok_or(InitError::Missing {
        table: "creature_equip_template selected reference",
        id,
    })?;
    if row.item_entries[1] != 0 || row.item_entries[2] != 0 {
        return Err(InitError::Unsupported(
            "offhand or ranged initial equipment",
        ));
    }
    let mainhand = if row.item_entries[0] == 0 {
        None
    } else {
        let item = items[&row.item_entries[0]];
        let info = u32::from(item.class as u8)
            | (u32::from(item.subclass as u8) << 8)
            | (u32::from(item.material as u32 as u8) << 16)
            | (u32::from(item.inventory_type as u8) << 24);
        Some(InitialVirtualItem {
            item_entry: item.entry,
            display_id: item.display_id,
            info: [info, u32::from(item.sheath as u8)],
            has_weapon: info & 0xff == 2,
        })
    };
    Ok(InitialEquipment {
        equipment_template_id: id,
        mainhand,
    })
}
