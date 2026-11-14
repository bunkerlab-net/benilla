// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed World::LoadGraveyardZones query operands, never recorded results.
use super::spawns::{columns, fill, position, uint};
use super::*;
use crate::native::source_execution::GraveyardReferences;
use std::collections::BTreeSet;

fn reference_environment_order(rows: &mut [[u32; 4]]) {
    // The frozen MyISAM table is recreated from db-snapshot.py:9's
    // --order-by-primary dump. Its PK is (id,ghost_loc,link_kind),
    // sql/base/mangos.sql:1853-1859. This is reference-environment behavior,
    // not a SELECT-without-ORDER-BY guarantee; preserve it before multimap insert.
    rows.sort_unstable_by_key(|row| [row[0], row[1], row[2]]);
}

impl VerifiedCatalog {
    pub(crate) fn execution_graveyard_operands(
        &mut self,
    ) -> Result<(Vec<[u32; 4]>, GraveyardReferences)> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, "game_graveyard_zone")?;
        let positions = ["id", "ghost_loc", "link_kind", "faction"]
            .map(|name| position(&cols, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut rows = Vec::new();
        self.scan("game_graveyard_zone", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let mut values = [0; 4];
            for (value, &pos) in values.iter_mut().zip(&positions) {
                *value = uint(cells[pos])?;
            }
            rows.push(values);
            check(rows.len() <= DECODED / 16, "graveyards.row_limit")
        })?;
        reference_environment_order(&mut rows);
        // World.cpp:904,2730-2733, SQLStorages.cpp:43,59: SQLStorage has
        // no graveyard-specific validation, and lookup tests only the ID slot.
        let cols = columns(&schema, "world_safe_locs")?;
        check(cols.len() == 7, "graveyards.safe_locations_layout")?;
        let id = position(&cols, "id")?;
        let mut safe_locations = BTreeSet::new();
        self.scan("world_safe_locs", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            safe_locations.insert(uint(cells[id])?);
            check(
                safe_locations.len() <= DECODED / 4,
                "graveyards.reference_limit",
            )
        })?;
        // GetAreaEntryByAreaID is a direct DBC lookup; DBCStores.cpp:671-674.
        let areas = self
            .execution_terrain_dbc("AreaTable.dbc")?
            .into_iter()
            .map(|row| row[0])
            .collect();
        let maps = self
            .execution_map_entries()?
            .into_iter()
            .map(|row| row.0)
            .collect();
        Ok((
            rows,
            GraveyardReferences {
                safe_locations,
                areas,
                maps,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn restored_myisam_rows_follow_composite_primary_key_not_export_ordinal() {
        let mut rows = [
            [90, 264, 0, 469],
            [91, 141, 0, 0],
            [90, 141, 1, 67],
            [90, 141, 0, 469],
        ];
        super::reference_environment_order(&mut rows);
        assert_eq!(
            rows,
            [
                [90, 141, 0, 469],
                [90, 141, 1, 67],
                [90, 264, 0, 469],
                [91, 141, 0, 0],
            ]
        );
    }
}
