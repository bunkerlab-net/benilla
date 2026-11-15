// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! OutdoorPvPMgr.cpp:29-72, OutdoorPvPSI.cpp:29-34, OutdoorPvPEP.cpp:28-42.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutdoorPvpScript {
    Silithus,
    EasternPlaguelands,
}
#[derive(Debug, PartialEq, Eq)]
pub struct SilithusPvp {
    pub resources: [u32; 2],
    pub owner: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub struct PlaguelandsPvp {
    pub towers: [u32; 2],
    pub tower_world_states: [u32; 4],
    pub tower_owners: [u32; 4],
}
#[derive(Debug)]
pub struct SourceOutdoorPvp {
    pub silithus: Option<SilithusPvp>,
    pub plaguelands: Option<PlaguelandsPvp>,
    pub update_interval: u32,
    initialized: bool,
}
impl Default for SourceOutdoorPvp {
    fn default() -> Self {
        // OutdoorPvPMgr.h:27, OutdoorPvPMgr.cpp:31-32.
        Self {
            silithus: None,
            plaguelands: None,
            update_interval: 60_000,
            initialized: false,
        }
    }
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OutdoorPvpInitCensus {
    pub scripts: u8,
    pub maps_visited: usize,
    pub map_messages: usize,
}
impl SourceOutdoorPvp {
    pub fn config(bytes: &[u8]) -> Result<[bool; 2]> {
        let mut values = [false; 2];
        for (value, key) in values
            .iter_mut()
            .zip(["OutdoorPvp.SIEnabled", "OutdoorPvp.EPEnabled"])
        {
            let raw = spawn_loader_config::assignment(bytes, key)?.1;
            *value =
                raw == "1" || raw.eq_ignore_ascii_case("true") || raw.eq_ignore_ascii_case("yes");
        }
        Ok(values)
    }
    /// World.cpp:1385 precedes MapManager::Initialize at :1389. Enumerate current maps.
    pub fn initialize(
        &mut self,
        enabled: [bool; 2],
        cause: u64,
        world: &mut SourceMessager,
        maps: &mut BTreeMap<MapIdentity, SourceMessager>,
    ) -> Result<OutdoorPvpInitCensus> {
        require(
            world.owner() == MessageOwner::World,
            "execution.outdoor_pvp.world_binding",
        )?;
        let mut census = OutdoorPvpInitCensus::default();
        if enabled[0] {
            self.silithus = Some(SilithusPvp {
                resources: [0; 2],
                owner: 0,
            });
            census.scripts += 1;
        }
        if enabled[1] {
            self.plaguelands = Some(PlaguelandsPvp {
                towers: [0; 2],
                tower_world_states: [2352, 2355, 2361, 2353],
                tower_owners: [0; 4],
            });
            // OutdoorPvP.cpp:34 queues first. No mutation until World.cpp:1554 drains it.
            world.enqueue_graveyard_team(cause, 927, 139, 1)?;
            // OutdoorPvP.cpp:35, MapManager.cpp:396-403 visits only current mapId=0 maps.
            for (map, messages) in maps {
                if map.map == 0 {
                    census.maps_visited += 1;
                    require(
                        messages.owner() == MessageOwner::Map(*map),
                        "execution.outdoor_pvp.map_binding",
                    )?;
                    messages.enqueue_graveyard_team(cause, 927, 139, 1)?;
                    census.map_messages += 1;
                }
            }
            census.scripts += 1;
        }
        self.initialized = true;
        Ok(census)
    }
    pub fn get_script(&self, zone: u32) -> Result<Option<OutdoorPvpScript>> {
        require(
            self.initialized,
            "execution.gameobject.loaded_outdoor_pvp_script_owner",
        )?;
        Ok(match zone {
            1377 => self.silithus.as_ref().map(|_| OutdoorPvpScript::Silithus),
            139 => self
                .plaguelands
                .as_ref()
                .map(|_| OutdoorPvpScript::EasternPlaguelands),
            _ => None,
        })
    }
}
#[cfg(test)]
#[path = "outdoor_pvp_tests.rs"]
mod tests;
