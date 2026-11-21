// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const BATTLEGROUND_TEMPLATE_SQL: &str = "SELECT id, MinPlayersPerTeam,MaxPlayersPerTeam,MinLvl,MaxLvl,AllianceStartLoc,HordeStartLoc,StartMaxDist,PlayerSkinReflootId FROM battleground_template";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BattleGroundRow {
    pub type_id: u32,
    pub min_per_team: u32,
    pub max_per_team: u32,
    pub min_level: u32,
    pub max_level: u32,
    pub alliance_start: u32,
    pub horde_start: u32,
    pub max_start_distance: f32,
    pub player_skin_refloot: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BattleGroundLoadNotice {
    EmptyTemplates,
    MissingAllianceStart { type_id: u32, id: u32 },
    MissingHordeStart { type_id: u32, id: u32 },
    MissingMap { type_id: u32, id: u32 },
    MissingReferenceLoot { type_id: u32, id: u32 },
}

pub trait BattleGroundInputs {
    fn templates(&mut self, _sql: &str) -> Result<Vec<BattleGroundRow>> {
        Err(refuse(
            "execution.battlegrounds.BattleGroundMgr_437.template_sql_owner",
        ))
    }
    fn safe_locations(&self) -> Option<&BTreeMap<u32, SourceWorldSafeLoc>> {
        None
    }
    fn map_name(&mut self, _map: u32) -> Result<Option<Box<[u8]>>> {
        Err(refuse(
            "execution.battlegrounds.BattleGroundMgr_502.map_store_owner",
        ))
    }
    fn notice(&mut self, notice: BattleGroundLoadNotice);
}

#[derive(Debug, PartialEq, Eq)]
pub enum BattleGroundMode {
    Base,
    AlteracValley {
        mine_yell_ms: u32,
        honor_map_complete: u32,
        rep_tower_destruction: u32,
        rep_captain: u32,
        rep_boss: u32,
        rep_owned_grave: u32,
        rep_owned_mine: u32,
        rep_survive_captain: u32,
        rep_survive_tower: u32,
    },
    WarsongGulch {
        rep_capture: u32,
        honor_win_kills: u32,
        honor_end_kills: u32,
    },
    ArathiBasin {
        informed_near_victory: bool,
        honor_ticks: u32,
        reputation_ticks: u32,
    },
}

#[derive(Debug)]
pub struct SourceBattleGroundTemplate {
    pub mode: BattleGroundMode,
    pub type_id: u32,
    pub map_id: u32,
    pub map: Option<MapIdentity>,
    pub client_instance_id: u32,
    pub bracket_id: i32,
    pub name: Box<[u8]>,
    pub status: u8,
    pub winner: u8,
    pub end_time: i32,
    pub start_time: u32,
    pub start_delay_ms: i32,
    pub start_delay_times_ms: [u32; 4],
    pub start_message_ids: [u32; 4],
    pub start_locations: [[f32; 4]; 2],
    pub max_start_distance: f32,
    pub valid_start_position_timer: u32,
    pub min_per_team: u32,
    pub max_per_team: u32,
    pub min_players: u32,
    pub max_players: u32,
    pub invited: [u32; 2],
    pub players: [u32; 2],
    pub raids: [Option<u64>; 2],
    pub min_level: u32,
    pub max_level: u32,
    pub events: u8,
    pub premature_countdown: bool,
    pub premature_countdown_timer: u32,
    pub buff_change: bool,
    pub has_free_slot_queue: bool,
    pub player_skin_refloot: u32,
}

impl SourceBattleGroundTemplate {
    // BattleGround.cpp:186-253 initializes the shared template state.
    fn new() -> Self {
        Self {
            mode: BattleGroundMode::Base,
            type_id: 0,
            map_id: 0,
            map: None,
            client_instance_id: 0,
            bracket_id: -1,
            name: Box::default(),
            status: 0,
            winner: 2,
            end_time: 0,
            start_time: 0,
            start_delay_ms: 0,
            start_delay_times_ms: [120_000, 60_000, 30_000, 0],
            start_message_ids: [0, 602, 603, 604],
            start_locations: [[0.0; 4]; 2],
            max_start_distance: 0.0,
            valid_start_position_timer: 0,
            min_per_team: 0,
            max_per_team: 0,
            min_players: 0,
            max_players: 0,
            invited: [0; 2],
            players: [0; 2],
            raids: [None; 2],
            min_level: 0,
            max_level: 0,
            events: 0,
            premature_countdown: false,
            premature_countdown_timer: 0,
            buff_change: false,
            has_free_slot_queue: false,
            player_skin_refloot: 0,
        }
    }

    // BattleGroundAV.cpp:30-36 and BattleGroundAV.h:27.
    fn alterac_valley() -> Self {
        Self {
            mode: BattleGroundMode::AlteracValley {
                mine_yell_ms: 25_000,
                honor_map_complete: 0,
                rep_tower_destruction: 0,
                rep_captain: 0,
                rep_boss: 0,
                rep_owned_grave: 0,
                rep_owned_mine: 0,
                rep_survive_captain: 0,
                rep_survive_tower: 0,
            },
            start_message_ids: [0, 784, 785, 786],
            ..Self::new()
        }
    }

    // BattleGroundWS.cpp:28-35.
    fn warsong_gulch() -> Self {
        Self {
            mode: BattleGroundMode::WarsongGulch {
                rep_capture: 0,
                honor_win_kills: 0,
                honor_end_kills: 0,
            },
            start_message_ids: [0, 602, 603, 604],
            ..Self::new()
        }
    }

    // BattleGroundAB.cpp:29-35.
    fn arathi_basin() -> Self {
        Self {
            mode: BattleGroundMode::ArathiBasin {
                informed_near_victory: false,
                honor_ticks: 0,
                reputation_ticks: 0,
            },
            start_message_ids: [0, 661, 662, 663],
            ..Self::new()
        }
    }

    // BattleGround.h:281 returns zero while the template has no map.
    pub fn instance_id(&self) -> u32 {
        self.map.map_or(0, |map| map.instance)
    }
}

#[derive(Default)]
pub struct SourceBattleGroundMgr {
    templates: BTreeMap<u32, SourceBattleGroundTemplate>,
    used_refloot: BTreeSet<u32>,
}

impl SourceBattleGroundMgr {
    pub fn template(&self, type_id: u32) -> Option<&SourceBattleGroundTemplate> {
        self.templates.get(&type_id)
    }

    pub fn used_refloot(&self) -> &BTreeSet<u32> {
        &self.used_refloot
    }

    // BattleGroundMgr.cpp:433-531 loads and registers templates in query order.
    pub fn create_initial_battlegrounds(
        &mut self,
        reference_loot: Option<&SourceReferenceLootStore>,
        inputs: &mut impl BattleGroundInputs,
    ) -> Result<usize> {
        let rows = inputs.templates(BATTLEGROUND_TEMPLATE_SQL)?;
        if rows.is_empty() {
            inputs.notice(BattleGroundLoadNotice::EmptyTemplates);
            return Ok(0);
        }
        let mut count = 0;
        for mut row in rows {
            let type_id = row.type_id;
            // BattleGroundDefines.h:50-57 limits the unscoped enum to 0..=3.
            require(
                type_id < 4,
                "execution.battlegrounds.BattleGroundMgr_457.type_range",
            )?;
            let safe_locations = inputs.safe_locations().ok_or_else(|| {
                refuse("execution.battlegrounds.BattleGroundMgr_469.safe_location_store_owner")
            })?;
            let Some(&alliance) = safe_locations.get(&row.alliance_start) else {
                inputs.notice(BattleGroundLoadNotice::MissingAllianceStart {
                    type_id,
                    id: row.alliance_start,
                });
                continue;
            };
            let Some(&horde) = safe_locations.get(&row.horde_start) else {
                inputs.notice(BattleGroundLoadNotice::MissingHordeStart {
                    type_id,
                    id: row.horde_start,
                });
                continue;
            };
            let map_id = map_id_by_type(type_id);
            let Some(name) = inputs.map_name(map_id)? else {
                inputs.notice(BattleGroundLoadNotice::MissingMap {
                    type_id,
                    id: map_id,
                });
                continue;
            };
            if row.player_skin_refloot != 0 {
                let store = reference_loot.ok_or_else(|| {
                    refuse("execution.battlegrounds.BattleGroundMgr_512.reference_loot_owner")
                })?;
                let id = row.player_skin_refloot;
                if !store.have_loot_for(id) {
                    row.player_skin_refloot = 0;
                    inputs.notice(BattleGroundLoadNotice::MissingReferenceLoot { type_id, id });
                }
            }
            if row.player_skin_refloot != 0 {
                self.used_refloot.insert(row.player_skin_refloot);
            }
            if self.create_battleground(row, map_id, name, [alliance.position, horde.position]) != 0
            {
                count += 1;
            }
        }
        Ok(count)
    }

    // BattleGroundMgr.cpp:391-420 sets template state after its constructor.
    fn create_battleground(
        &mut self,
        row: BattleGroundRow,
        map: u32,
        name: Box<[u8]>,
        starts: [[f32; 4]; 2],
    ) -> u32 {
        let mut template = match row.type_id {
            1 => SourceBattleGroundTemplate::alterac_valley(),
            2 => SourceBattleGroundTemplate::warsong_gulch(),
            3 => SourceBattleGroundTemplate::arathi_basin(),
            _ => SourceBattleGroundTemplate::new(),
        };
        template.map_id = map;
        template.type_id = row.type_id;
        template.min_per_team = row.min_per_team;
        template.max_per_team = row.max_per_team;
        template.min_players = row.min_per_team.wrapping_mul(2);
        template.max_players = row.max_per_team.wrapping_mul(2);
        template.name = name;
        template.start_locations = starts;
        template.max_start_distance = row.max_start_distance;
        template.min_level = row.min_level;
        template.max_level = row.max_level;
        template.player_skin_refloot = row.player_skin_refloot;
        self.add_battleground(template);
        row.type_id
    }

    // BattleGroundMgr.cpp:423-428 owns the instance-zero entry by type.
    fn add_battleground(&mut self, template: SourceBattleGroundTemplate) {
        self.templates.insert(template.type_id, template);
    }
}

// BattleGroundDefines.h:70-82 maps template types to Map.dbc keys.
fn map_id_by_type(type_id: u32) -> u32 {
    match type_id {
        1 => 30,
        2 => 489,
        3 => 529,
        _ => 0,
    }
}

#[cfg(test)]
#[path = "battlegrounds_tests.rs"]
mod tests;
