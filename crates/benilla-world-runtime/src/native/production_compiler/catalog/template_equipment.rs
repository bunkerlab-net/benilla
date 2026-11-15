// SPDX-License-Identifier: GPL-3.0-or-later
//! ObjectMgr.cpp:1563-1600 fixes item slots but never erases an equipment key.
use super::*;

fn projection(table: &str, column: &str) -> Result<()> {
    check(
        (table, column) == ("creature_equip_template", "entry"),
        "templates.equipment_projection",
    )
}
impl VerifiedCatalog {
    pub(crate) fn execution_equipment_template_ids(
        &mut self,
        table: &str,
        column: &str,
    ) -> Result<std::collections::BTreeSet<u32>> {
        projection(table, column)?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let field = spawns::position(&spawns::columns(&schema, table)?, column)?;
        let mut ids = std::collections::BTreeSet::new();
        self.scan(table, 0, 256, |row| {
            let mut cells = [None; 256];
            spawns::fill(&row, &mut cells)?;
            ids.insert(spawns::uint(cells[field])?);
            Ok(())
        })?;
        Ok(ids)
    }
}
#[test]
fn equipment_reader_refuses_other_relations_and_columns() {
    projection("creature_equip_template", "entry").unwrap();
    for (table, column) in [
        ("creature_template", "Entry"),
        ("creature_equip_template", "equipentry1"),
        ("spell_template", "id"),
    ] {
        assert_eq!(
            projection(table, column).unwrap_err().id,
            "templates.equipment_projection"
        );
    }
}
