// SPDX-License-Identifier: GPL-3.0-or-later
//! Narrow retained operands for corpse display construction, not admission.
use super::*;
use benilla_sim::corpse_appearance::EquippedItem;
pub(crate) struct CorpsePlayerCreationRow {
    pub race: u32,
    pub class: u32,
    pub map: u32,
    pub position: [u32; 4],
}
impl VerifiedCatalog {
    pub(crate) fn execution_corpse_player_rows(&mut self) -> Result<Vec<CorpsePlayerCreationRow>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, "playercreateinfo")?;
        let indices = [
            "race",
            "class",
            "map",
            "position_x",
            "position_y",
            "position_z",
            "orientation",
        ]
        .map(|name| spawns::position(&columns, name))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
        let mut rows = Vec::new();
        let count = self
            .relations
            .get("playercreateinfo")
            .ok_or_else(|| fail("catalog.table"))?
            .rows;
        check(count <= 2_000_000, "corpse.player_info_limit")?;
        for start in (0..count).step_by(256) {
            self.scan("playercreateinfo", start, 256, |row| {
                let mut cells = [None; 256];
                spawns::fill(&row, &mut cells)?;
                rows.push(CorpsePlayerCreationRow {
                    race: spawns::uint(cells[indices[0]])?,
                    class: spawns::uint(cells[indices[1]])?,
                    map: spawns::uint(cells[indices[2]])?,
                    position: [
                        spawns::floating(cells[indices[3]])?.to_bits(),
                        spawns::floating(cells[indices[4]])?.to_bits(),
                        spawns::floating(cells[indices[5]])?.to_bits(),
                        spawns::floating(cells[indices[6]])?.to_bits(),
                    ],
                });
                Ok(())
            })?;
        }
        Ok(rows)
    }
    pub(crate) fn execution_corpse_item(&mut self, id: u32) -> Result<Option<EquippedItem>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, "item_template")?;
        let display = spawns::position(&columns, "displayid")?;
        let inventory = spawns::position(&columns, "InventoryType")?;
        let mut item = None;
        spawns::visit_key(self, "item_template", id, |row| {
            check(item.is_none(), "corpse.duplicate_item")?;
            let mut cells = [None; 256];
            spawns::fill(&row, &mut cells)?;
            let inventory_type = spawns::uint(cells[inventory])?;
            // Execute the effective ItemPrototype correction, not the raw invalid value; ObjectMgr.cpp:2604-2607, ItemPrototype.h:140.
            item = Some(corrected_item(
                spawns::uint(cells[display])?,
                inventory_type,
            ));
            Ok(())
        })?;
        Ok(item)
    }
}

fn corrected_item(display_info_id: u32, inventory_type: u32) -> EquippedItem {
    EquippedItem {
        display_info_id,
        inventory_type: if inventory_type >= 29 {
            0
        } else {
            inventory_type
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corpse_item_display_keeps_bits_and_inventory_correction_uses_source_limit() {
        assert_eq!(
            corrected_item(u32::MAX, 28),
            EquippedItem {
                display_info_id: u32::MAX,
                inventory_type: 28
            }
        );
        assert_eq!(
            corrected_item(7, 29),
            EquippedItem {
                display_info_id: 7,
                inventory_type: 0
            }
        );
        assert_eq!(
            corrected_item(7, u32::MAX),
            EquippedItem {
                display_info_id: 7,
                inventory_type: 0
            }
        );
    }
}
