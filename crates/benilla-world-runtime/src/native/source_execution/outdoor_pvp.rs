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

#[derive(Clone, Copy, Debug)]
pub struct CapturePointSlider {
    pub value: f32,
    pub locked: bool,
}

impl OutdoorPvpScript {
    pub(super) fn handle_gameobject_create<
        D: GridDefinitions,
        P: GridProductOwner,
        F: GameObjectFactoryOwner<D, P>,
    >(
        self,
        owner: &mut F,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        // OutdoorPvP.cpp:80-92. OutdoorPvPSI inherits this handler.
        let template = &owner.gameobjects().get(object)?.template;
        if template.kind == 29 {
            let entry = template.entry;
            let slider =
                owner
                    .gameobject_capture_point_slider(entry)?
                    .unwrap_or(CapturePointSlider {
                        value: 50.0,
                        locked: false,
                    });
            gameobject_lifecycle::set_source_capture_point_slider(owner, object, slider, context)?;
        }
        if self == Self::Silithus {
            return Ok(());
        }
        // OutdoorPvPEP.cpp:113-148 reads the live entry after the base callback.
        let entry = owner.gameobjects().get(object)?.template.entry;
        let guid = |id| context.objects.get(object)?.guid.ok_or_else(|| refuse(id));
        let tower = match entry {
            181899 => 0,
            182096 => 1,
            182097 => 2,
            182098 => 3,
            182106 => {
                let Some(tower) = banner_tower(context.objects.get(object)?)? else {
                    return Ok(());
                };
                tower
            }
            181682 => {
                let guid =
                    guid("execution.outdoor_pvp.OutdoorPvPEP_143.alliance_shrine_guid_owner")?;
                owner
                    .gameobject_outdoor_pvp()?
                    .plaguelands_mut()?
                    .lordaeron_shrine_alliance = guid;
                return Ok(());
            }
            181955 => {
                let guid = guid("execution.outdoor_pvp.OutdoorPvPEP_146.horde_shrine_guid_owner")?;
                owner
                    .gameobject_outdoor_pvp()?
                    .plaguelands_mut()?
                    .lordaeron_shrine_horde = guid;
                return Ok(());
            }
            _ => return Ok(()),
        };
        // OutdoorPvPEP.cpp:346-349 appends the GUID before setting the art kit.
        let guid = guid("execution.outdoor_pvp.OutdoorPvPEP_348.banner_guid_owner")?;
        let live = owner.gameobject_outdoor_pvp()?.plaguelands_mut()?;
        live.tower_banners[tower].push(guid);
        let art_kit = banner_art_kit(live.tower_owners[tower]);
        owner
            .gameobjects()
            .get_mut(object)?
            .set_art_kit(context.objects, art_kit)
    }
}

// OutdoorPvPEP.h:157-163. Northpass, Crownguard, Eastwall, Plaguewood.
pub(super) const PLAGUELANDS_TOWER_CENTRES: [[f32; 2]; 4] = [
    [3181.08, -4379.36],
    [1860.85, -3731.23],
    [2574.51, -4794.89],
    [2962.71, -3042.31],
];

// Object.cpp:1310-1314,1331. DIST_CALC_NONE returns the squared distance.
pub(super) fn point_xy_distance_squared(from: [f32; 2], to: [f32; 2]) -> f32 {
    let dx = from[0] - to[0];
    let dy = from[1] - to[1];
    dx * dx + dy * dy
}

// Object.cpp:1352-1359. Use separate binary32 operations and a strict limit.
fn is_within_dist_2d(from: [f32; 2], to: [f32; 2], distance: f32, reach: f32) -> bool {
    let squared = point_xy_distance_squared(from, to);
    let max = distance + reach;
    squared < max * max
}

// OutdoorPvPEP.cpp:133-140. Stop after the first matching tower.
fn first_banner_tower(within: impl FnMut([f32; 2]) -> bool) -> Option<usize> {
    PLAGUELANDS_TOWER_CENTRES.iter().copied().position(within)
}

fn banner_tower(object: &SourceObject) -> Result<Option<usize>> {
    let SourcePose([x, y, ..]) = object
        .pose
        .ok_or_else(|| refuse("execution.outdoor_pvp.Object_1312.point_position_owner"))?;
    // Object.h:926. GameObject inherits the WorldObject reach.
    require(
        object.kind == SourceObjectKind::GameObject,
        "execution.outdoor_pvp.Object_1355.GetCombatReach_owner",
    )?;
    let reach = 0.0_f32;
    let from = [f32::from_bits(x), f32::from_bits(y)];
    Ok(first_banner_tower(|centre| {
        is_within_dist_2d(from, centre, 50.0, reach)
    }))
}

// OutdoorPvP.h:34.
pub(super) const CAPTURE_ARTKIT_NEUTRAL: u32 = 21;

// OutdoorPvP.cpp:172-183 and OutdoorPvP.h:32-34 select team art or neutral art.
fn banner_art_kit(team: u32) -> u32 {
    match team {
        469 => 2,
        67 => 1,
        _ => CAPTURE_ARTKIT_NEUTRAL,
    }
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
    pub tower_banners: [Vec<SourceGuid>; 4],
    pub lordaeron_shrine_alliance: SourceGuid,
    pub lordaeron_shrine_horde: SourceGuid,
    pub flight_master: SourceGuid,
    pub soldiers: Vec<SourceGuid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaguelandsCreatureCreate {
    NoCase,
    Soldier,
}

impl PlaguelandsPvp {
    // OutdoorPvPEP.cpp:95-111; OutdoorPvPEP.h:54,70-75.
    pub(super) fn handle_creature_create(
        &mut self,
        entry: u32,
        creature: &SourceObject,
    ) -> Result<PlaguelandsCreatureCreate> {
        match entry {
            17209 => {
                self.flight_master = creature.guid.ok_or_else(|| {
                    refuse("execution.outdoor_pvp.OutdoorPvPEP_100.flight_master_guid_owner")
                })?;
                Err(refuse(
                    "execution.outdoor_pvp.OutdoorPvPEP_101.setFaction_owner",
                ))
            }
            17635 | 17647 | 17995 | 17996 => {
                let guid = creature.guid.ok_or_else(|| {
                    refuse("execution.outdoor_pvp.OutdoorPvPEP_108.soldier_guid_owner")
                })?;
                self.soldiers.push(guid);
                Ok(PlaguelandsCreatureCreate::Soldier)
            }
            _ => Ok(PlaguelandsCreatureCreate::NoCase),
        }
    }
}
#[derive(Debug)]
pub struct SourceOutdoorPvp {
    pub silithus: Option<SilithusPvp>,
    pub plaguelands: Option<PlaguelandsPvp>,
    pub update_interval: u32,
    capture_points: BTreeMap<u32, CapturePointSlider>,
    initialized: bool,
}
impl Default for SourceOutdoorPvp {
    fn default() -> Self {
        // OutdoorPvPMgr.h:27, OutdoorPvPMgr.cpp:31-32.
        Self {
            silithus: None,
            plaguelands: None,
            update_interval: 60_000,
            capture_points: BTreeMap::new(),
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
                // OutdoorPvPEP.h:237-242 and ObjectGuid.h:85 construct empty GUIDs and lists.
                tower_banners: Default::default(),
                lordaeron_shrine_alliance: SourceGuid::default(),
                lordaeron_shrine_horde: SourceGuid::default(),
                // OutdoorPvPEP.h:236,240 default-constructs these members.
                flight_master: SourceGuid::default(),
                soldiers: Vec::new(),
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

    pub(super) fn plaguelands_mut(&mut self) -> Result<&mut PlaguelandsPvp> {
        self.plaguelands
            .as_mut()
            .ok_or_else(|| refuse("execution.outdoor_pvp.OutdoorPvPEP.plaguelands_script_owner"))
    }

    // OutdoorPvPMgr.h:65,88-89. The key is the template entry, not the spawn GUID.
    pub fn capture_point_slider(&self, entry: u32) -> Option<CapturePointSlider> {
        self.capture_points.get(&entry).copied()
    }

    pub fn set_capture_point_slider(&mut self, entry: u32, slider: CapturePointSlider) {
        self.capture_points.insert(entry, slider);
    }
}
#[cfg(test)]
#[path = "outdoor_pvp_tests.rs"]
mod tests;
