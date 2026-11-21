// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed World::LoadGraveyardZones query operands, never recorded results.
use super::spawns::{columns, fill, floating, position, uint};
use super::*;
use crate::native::source_execution::{GraveyardReferences, SourceWorldSafeLoc};

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
        // World.cpp:904,2730-2733 and SQLStorages.cpp:43,59 load one safe-location store.
        let cols = columns(&schema, "world_safe_locs")?;
        check(cols.len() == 7, "graveyards.safe_locations_layout")?;
        let mut positions = [0; 6];
        for (at, name) in positions.iter_mut().zip(["id", "map", "x", "y", "z", "o"]) {
            *at = position(&cols, name)?;
        }
        let mut safe_locations = BTreeMap::new();
        self.scan("world_safe_locs", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let [id, map, x, y, z, o] = positions.map(|at| cells[at]);
            safe_locations.insert(
                uint(id)?,
                SourceWorldSafeLoc {
                    map_id: uint(map)?,
                    position: [floating(x)?, floating(y)?, floating(z)?, floating(o)?],
                },
            );
            check(
                safe_locations.len() <= DECODED / (4 + std::mem::size_of::<SourceWorldSafeLoc>()),
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
