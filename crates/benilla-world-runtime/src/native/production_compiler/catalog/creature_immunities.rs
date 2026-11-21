// SPDX-License-Identifier: GPL-3.0-or-later
//! ObjectMgr.cpp:948 fixed creature_immunities projection.
use super::*;
const COLUMNS: [&str; 4] = ["Entry", "SetId", "Type", "Value"];
fn projection(table: &str, columns: &[String]) -> Result<()> {
    check(
        table == "creature_immunities",
        "creature_immunities.fixed_relation",
    )?;
    check(
        columns.iter().map(String::as_str).eq(COLUMNS),
        "creature_immunities.fixed_columns",
    )
}
impl VerifiedCatalog {
    pub(crate) fn execution_creature_immunities(&mut self) -> Result<Vec<[u32; 4]>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        projection(
            "creature_immunities",
            &spawns::columns(&schema, "creature_immunities")?,
        )?;
        let mut rows = Vec::new();
        self.scan("creature_immunities", 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            rows.push([
                spawns::uint(cells[0])?,
                spawns::uint(cells[1])?,
                spawns::uint(cells[2])?,
                spawns::uint(cells[3])?,
            ]);
            check(
                rows.len() <= DECODED / std::mem::size_of::<[u32; 4]>(),
                "creature_immunities.view_limit",
            )
        })?;
        // sql/base/mangos.sql:1463 PRIMARY KEY(Entry,SetId,Type,Value), all unsigned.
        // The --order-by-primary snapshot restore defines source row order.
        rows.sort_unstable();
        Ok(rows)
    }
}
#[test]
fn creature_immunities_reader_refuses_other_relations_and_columns() {
    let mut columns = COLUMNS.map(str::to_owned).to_vec();
    projection("creature_immunities", &columns).unwrap();
    assert_eq!(
        projection("creature_template", &columns).unwrap_err().id,
        "creature_immunities.fixed_relation"
    );
    columns.swap(0, 1);
    assert_eq!(
        projection("creature_immunities", &columns).unwrap_err().id,
        "creature_immunities.fixed_columns"
    );
    columns.pop();
    assert_eq!(
        projection("creature_immunities", &columns).unwrap_err().id,
        "creature_immunities.fixed_columns"
    );
}
