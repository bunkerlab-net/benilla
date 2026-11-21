// SPDX-License-Identifier: GPL-3.0-or-later
//! BattleGroundMgr.cpp:437 reads world SQL and World.cpp:904 loads safe locations.
use super::spawns::{columns, fill, floating, position, uint};
use super::*;
use crate::native::source_execution::BattleGroundRow;

impl VerifiedCatalog {
    pub fn execution_battleground_rows(&mut self) -> Result<Vec<BattleGroundRow>> {
        let table = "battleground_template";
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let mut positions = [0; 9];
        for (at, name) in positions.iter_mut().zip([
            "id",
            "MinPlayersPerTeam",
            "MaxPlayersPerTeam",
            "MinLvl",
            "MaxLvl",
            "AllianceStartLoc",
            "HordeStartLoc",
            "StartMaxDist",
            "PlayerSkinReflootId",
        ]) {
            *at = position(&cols, name)?;
        }
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |raw| {
            let mut cells = [None; 256];
            fill(&raw, &mut cells)?;
            let [id, min_team, max_team, min_level, max_level, alliance, horde, distance, loot] =
                positions.map(|at| cells[at]);
            rows.push(BattleGroundRow {
                type_id: uint(id)?,
                min_per_team: uint(min_team)?,
                max_per_team: uint(max_team)?,
                min_level: uint(min_level)?,
                max_level: uint(max_level)?,
                alliance_start: uint(alliance)?,
                horde_start: uint(horde)?,
                max_start_distance: floating(distance)?,
                player_skin_refloot: uint(loot)?,
            });
            check(
                rows.len() <= DECODED / std::mem::size_of::<BattleGroundRow>(),
                "battlegrounds.row_limit",
            )
        })?;
        // The restored MyISAM table is inserted in primary-key order.
        rows.sort_unstable_by_key(|row| row.type_id);
        Ok(rows)
    }
}
