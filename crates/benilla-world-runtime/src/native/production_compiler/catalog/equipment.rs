// SPDX-License-Identifier: GPL-3.0-or-later
//! SQLStorages.cpp:28,49 fixes the four-column creature_equip_template layout.
use super::*;
use crate::native::source_execution::{EquipmentItem, LoadedEquipmentRow, SqlTemplateRow};
const COLUMNS: [&str; 4] = ["entry", "equipentry1", "equipentry2", "equipentry3"];
fn projection(table: &str, columns: &[String]) -> Result<()> {
    check(
        table == "creature_equip_template" && columns.iter().map(String::as_str).eq(COLUMNS),
        "equipment.fixed_projection",
    )
}
const ITEM_COLUMNS: [&str; 7] = [
    "entry",
    "displayid",
    "class",
    "subclass",
    "Material",
    "InventoryType",
    "sheath",
];
fn item_projection(table: &str, columns: &[&str]) -> Result<()> {
    check(
        table == "item_template" && columns == ITEM_COLUMNS,
        "equipment.item_fixed_projection",
    )
}
impl VerifiedCatalog {
    pub(crate) fn execution_equipment_rows(&mut self) -> Result<Vec<LoadedEquipmentRow>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        projection(
            "creature_equip_template",
            &spawns::columns(&schema, "creature_equip_template")?,
        )?;
        let mut rows = Vec::new();
        self.scan("creature_equip_template", 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            let mut words = [0; 4];
            for (word, cell) in words.iter_mut().zip(cells) {
                *word = spawns::uint(cell)?;
            }
            rows.push(SqlTemplateRow {
                ordinal: raw.ordinal,
                words,
                strings: [],
            });
            check(
                rows.len() <= DECODED / std::mem::size_of::<LoadedEquipmentRow>(),
                "equipment.view_limit",
            )
        })?;
        // Reference environment: ClassicDB_1_12_1_z2815.sql:683 PRIMARY KEY(entry), :684 MyISAM; --order-by-primary pre-start-classicmangos.sql:161405-161413, sealed by seal16-full-01/reset.json:5-7.
        rows.sort_unstable_by_key(|row| row.words[0]);
        check(
            rows.windows(2)
                .all(|pair| pair[0].words[0] < pair[1].words[0]),
            "equipment.primary_key",
        )?;
        Ok(rows)
    }
    pub(crate) fn execution_equipment_items(&mut self) -> Result<BTreeMap<u32, EquipmentItem>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, "item_template")?;
        item_projection("item_template", &ITEM_COLUMNS)?;
        let mut positions = [0; 7];
        for (position, name) in positions.iter_mut().zip(ITEM_COLUMNS) {
            *position = spawns::position(&columns, name)?;
        }
        let mut items = BTreeMap::new();
        self.scan("item_template", 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            let mut words = [0; 7];
            for (word, &position) in words.iter_mut().zip(&positions) {
                *word = spawns::uint(cells[position])?;
            }
            let [entry, display, class, subclass, material, inventory_type, sheath] = words;
            check(
                items
                    .insert(
                        entry,
                        EquipmentItem {
                            display,
                            class,
                            subclass,
                            material,
                            inventory_type,
                            sheath,
                        },
                    )
                    .is_none(),
                "equipment.item_primary_key",
            )?;
            check(items.len() <= DECODED / 64, "equipment.item_view_limit")
        })?;
        Ok(items)
    }
}
#[test]
fn equipment_fixed_reader_refuses_every_other_table_and_layout() {
    let columns = COLUMNS.map(str::to_owned);
    projection("creature_equip_template", &columns).unwrap();
    for table in [
        "creature_template",
        "creature_equip_template_raw",
        "item_template",
    ] {
        assert_eq!(
            projection(table, &columns).unwrap_err().id,
            "equipment.fixed_projection"
        );
    }
    let mut swapped = columns.clone();
    swapped.swap(1, 2);
    assert_eq!(
        projection("creature_equip_template", &swapped)
            .unwrap_err()
            .id,
        "equipment.fixed_projection"
    );
    assert_eq!(
        projection("creature_equip_template", &columns[..3])
            .unwrap_err()
            .id,
        "equipment.fixed_projection"
    );
}
#[test]
fn virtual_item_reader_refuses_other_tables_and_columns() {
    item_projection("item_template", &ITEM_COLUMNS).unwrap();
    assert_eq!(
        item_projection("creature_equip_template", &ITEM_COLUMNS)
            .unwrap_err()
            .id,
        "equipment.item_fixed_projection"
    );
    let mut columns = ITEM_COLUMNS;
    columns[1] = "RandomProperty";
    assert_eq!(
        item_projection("item_template", &columns).unwrap_err().id,
        "equipment.item_fixed_projection"
    );
}
