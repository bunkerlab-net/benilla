// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed encounter-loader table operands, never recorded SQL results.
use super::spawns::{columns, fill, uint};
use super::*;
use crate::native::source_execution::{
    DungeonEncounterEntry, EncounterReferences, LoadedDungeonEncounters,
};
use std::collections::BTreeSet;

const TABLES: [(&str, &str); 4] = [
    (
        "instance_dungeon_encounters",
        "encounters.missing.instance_dungeon_encounters",
    ),
    (
        "instance_encounters",
        "encounters.missing.instance_encounters",
    ),
    ("creature_template", "encounters.missing.creature_template"),
    ("spell_template", "encounters.missing.spell_template"),
];
fn require_tables(mut present: impl FnMut(&str) -> bool) -> Result<()> {
    for (table, refusal) in TABLES {
        check(present(table), refusal)?;
    }
    Ok(())
}
impl VerifiedCatalog {
    pub(crate) fn execution_encounter_operands(
        &mut self,
    ) -> Result<(LoadedDungeonEncounters, Vec<[u32; 4]>, EncounterReferences)> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        // This reader has its own fixed allowlist. Absence is not an empty store.
        require_tables(|table| {
            self.relations.contains_key(table) && schema["tables"][table].is_object()
        })?;
        let cols = columns(&schema, "instance_dungeon_encounters")?;
        check(cols.len() == 24, "encounters.dungeon_layout")?;
        let mut dungeon = Vec::new();
        self.scan("instance_dungeon_encounters", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            dungeon.push(DungeonEncounterEntry {
                id: uint(cells[0])?,
                map: uint(cells[1])?,
                difficulty: uint(cells[2])?,
                data: uint(cells[3])?,
                index: uint(cells[4])?,
                complete_world_state: uint(cells[5])?,
                // Field.h:45-47 maps NULL to "", SQLStorageImpl.h:424 copies C strings.
                names: std::array::from_fn(|i| {
                    cells[i + 6]
                        .unwrap_or_default()
                        .split(|&b| b == 0)
                        .next()
                        .unwrap()
                        .to_vec()
                }),
                name_flags: uint(cells[22])?,
                icon: uint(cells[23])?,
            });
            check(dungeon.len() <= DECODED / 256, "encounters.row_limit")
        })?;
        let cols = columns(&schema, "instance_encounters")?;
        check(
            cols.iter().map(String::as_str).eq([
                "entry",
                "creditType",
                "creditEntry",
                "lastEncounterDungeon",
            ]),
            "encounters.instance_layout",
        )?;
        let mut rows = Vec::new();
        self.scan("instance_encounters", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            rows.push([
                uint(cells[0])?,
                uint(cells[1])?,
                uint(cells[2])?,
                uint(cells[3])?,
            ]);
            check(rows.len() <= DECODED / 16, "encounters.row_limit")
        })?;
        // MyISAM PK entry, pre-start-classicmangos.sql:561028-561040.
        // db-snapshot.py:9 restores --order-by-primary insertion order.
        rows.sort_unstable_by_key(|row| row[0]);
        let mut references = EncounterReferences {
            creatures: BTreeSet::new(),
            spells: BTreeSet::new(),
        };
        for (table, target) in [
            ("creature_template", &mut references.creatures),
            ("spell_template", &mut references.spells),
        ] {
            self.scan(table, 0, 256, |row| {
                let mut cells = [None; 256];
                fill(&row, &mut cells)?;
                target.insert(uint(cells[0])?);
                check(target.len() <= DECODED / 4, "encounters.reference_limit")
            })?;
        }
        Ok((LoadedDungeonEncounters::load(dungeon), rows, references))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn encounter_reader_refuses_each_missing_table_by_name_in_source_order() {
        for (missing, refusal) in super::TABLES {
            assert_eq!(
                super::require_tables(|table| table != missing)
                    .unwrap_err()
                    .id,
                refusal
            );
        }
        assert_eq!(
            super::require_tables(|_| false).unwrap_err().id,
            "encounters.missing.instance_dungeon_encounters"
        );
        super::require_tables(|_| true).unwrap();
    }
}
