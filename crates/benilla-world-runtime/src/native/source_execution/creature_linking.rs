// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureLinkingMgr.cpp:64-785. Passive gates and synchronous linked events.
use super::super::production_compiler::{LinkingCatalogRows, VerifiedCatalog};
use super::*;
use rustc_apfloat::{ieee::Single, Float};
use std::collections::{BTreeMap, BTreeSet};

#[path = "creature_linking_events.rs"]
mod events;
pub use events::*;

pub const LINKING_BOSS_DEAD: u16 = 0x0400;
pub const LINKING_BOSS_ALIVE: u16 = 0x0800;
const PASSIVE: u16 = LINKING_BOSS_DEAD | LINKING_BOSS_ALIVE;
const FOLLOW: u16 = 0x0200;
const REVERSE: u16 = 0x0002 | 0x0008 | FOLLOW;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkingTemplateRow {
    pub ordinal: u64,
    pub slave: u32,
    pub map: u32,
    pub master: u32,
    pub flags: u16,
    pub range: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkingGuidRow {
    pub ordinal: u64,
    pub slave: u32,
    pub master: u32,
    pub flags: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkingSpawnData {
    pub entry: u32,
    pub map: u32,
    pub respawn_xy: [u32; 2],
}
pub trait LinkingRows {
    fn next_template(&mut self) -> Result<Option<LinkingTemplateRow>>;
    fn next_guid(&mut self) -> Result<Option<LinkingGuidRow>>;
}
impl LinkingRows for LinkingCatalogRows {
    fn next_template(&mut self) -> Result<Option<LinkingTemplateRow>> {
        Ok(self
            .next_template()?
            .map(
                |(ordinal, [slave, map, master, flags, range])| LinkingTemplateRow {
                    ordinal,
                    slave,
                    map,
                    master,
                    flags: flags as u16,
                    range: range as u16,
                },
            ))
    }
    fn next_guid(&mut self) -> Result<Option<LinkingGuidRow>> {
        Ok(self
            .next_guid()?
            .map(|(ordinal, [slave, master, flags])| LinkingGuidRow {
                ordinal,
                slave,
                master,
                flags: flags as u16,
            }))
    }
}
/// A result produced by the actual reached LIMIT 2 query, not an answer input.
pub enum LinkingMasterQuery {
    NoResult,
    Rows(Vec<u32>),
}
/// CreatureLinkingMgr.cpp:236-249 over retained raw SQL rows, not effective
/// ObjectMgr spawns. F1 must still bind the actual reached query occurrence.
pub fn query_catalog_linking_master(
    catalog: &mut VerifiedCatalog,
    entry: u32,
    map: u32,
) -> Result<LinkingMasterQuery> {
    let guids = catalog.execution_linking_unique_master(entry, map)?;
    Ok(if guids.is_empty() {
        LinkingMasterQuery::NoResult
    } else {
        LinkingMasterQuery::Rows(guids)
    })
}
pub trait LinkingLoadOwner {
    fn template_exists(&mut self, entry: u32) -> Result<bool>;
    fn map_exists(&mut self, map: u32) -> Result<bool>;
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>>;
    fn query_unique_master(&mut self, entry: u32, map: u32) -> Result<LinkingMasterQuery>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LinkingInfo {
    pub map: u32,
    pub master: u32,
    pub master_guid: u32,
    pub flags: u16,
    pub range: u16,
}
impl LinkingInfo {
    pub fn spawning_linked(self) -> bool {
        self.flags & PASSIVE != 0 && (self.master_guid != 0 || self.range != 0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LinkingRejection {
    SlaveTemplate,
    MasterTemplate,
    Map,
    SlaveGuid,
    MasterGuid,
    DifferentMaps,
    Flags,
    SelfDespawnOnRespawn,
    MissingUniqueMaster,
    NonuniqueMaster,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkingRejectedRow {
    pub by_entry: bool,
    pub ordinal: u64,
    pub slave: u32,
    pub reason: LinkingRejection,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkingDefinitionsInventory {
    pub version: u32,
    pub loaded: bool,
    pub stopped: bool,
    pub entries: BTreeMap<u32, Vec<LinkingInfo>>,
    pub guids: BTreeMap<u32, Vec<LinkingInfo>>,
    pub rejected: Vec<LinkingRejectedRow>,
}
#[derive(Default)]
pub struct SourceCreatureLinking {
    entries: BTreeMap<u32, Vec<LinkingInfo>>,
    guids: BTreeMap<u32, Vec<LinkingInfo>>,
    event_entries: BTreeSet<u32>,
    event_guids: BTreeSet<u32>,
    rejected: Vec<LinkingRejectedRow>,
    loaded: bool,
    stopped: bool,
}
impl SourceCreatureLinking {
    pub fn inventory(&self) -> LinkingDefinitionsInventory {
        LinkingDefinitionsInventory {
            version: 1,
            loaded: self.loaded,
            stopped: self.stopped,
            entries: self.entries.clone(),
            guids: self.guids.clone(),
            rejected: self.rejected.clone(),
        }
    }
    // Equivalent-key vectors retain std::multimap insertion order; cpp:106,153,309-321.
    pub fn lookup(&self, entry: u32, database: u32, map: u32) -> Option<LinkingInfo> {
        self.guids
            .get(&database)
            .and_then(|v| v.first())
            .copied()
            .or_else(|| {
                self.entries
                    .get(&entry)?
                    .iter()
                    .find(|v| v.map == map)
                    .copied()
            })
    }
    pub fn is_master(&self, entry: u32) -> bool {
        self.event_entries.contains(&entry)
    }
    pub fn event_trigger(&self, entry: u32, database: u32, map: u32) -> bool {
        self.event_entries.contains(&entry)
            || self.event_guids.contains(&database)
            || self
                .lookup(entry, database, map)
                .is_some_and(|v| v.flags & REVERSE != 0)
    }
    pub fn ensure_loaded(&self) -> Result<()> {
        require(!self.stopped, "execution.linking.stopped")?;
        require(self.loaded, "execution.linking.loader_owner")
    }
    /// Uses retained query occurrences; phase0 verification is not sealed acceptance.
    pub fn load_catalog(
        &mut self,
        catalog: &mut VerifiedCatalog,
        occurrences: &Path,
        owner: &mut impl LinkingLoadOwner,
    ) -> Result<()> {
        require(!self.stopped, "execution.linking.stopped")?;
        let result = (|| {
            let mut rows = catalog.execution_linking_rows(occurrences)?;
            self.load(&mut rows, owner)
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn load(
        &mut self,
        rows: &mut impl LinkingRows,
        owner: &mut impl LinkingLoadOwner,
    ) -> Result<()> {
        require(!self.stopped, "execution.linking.stopped")?;
        self.entries.clear();
        self.guids.clear();
        self.event_entries.clear();
        self.event_guids.clear();
        self.rejected.clear();
        self.loaded = false;
        let result = self.load_rows(rows, owner);
        self.stopped = result.is_err();
        self.loaded = result.is_ok();
        result
    }
    fn reject(&mut self, by_entry: bool, ordinal: u64, slave: u32, reason: LinkingRejection) {
        self.rejected.push(LinkingRejectedRow {
            by_entry,
            ordinal,
            slave,
            reason,
        });
    }
    fn flags_valid(slave: u32, info: LinkingInfo) -> Option<LinkingRejection> {
        if info.flags == 0 || info.flags & 0x8000 != 0 {
            return Some(LinkingRejection::Flags);
        }
        if info.flags & 0x0100 != 0 && slave == info.master {
            return Some(LinkingRejection::SelfDespawnOnRespawn);
        }
        None
    }
    fn load_rows(
        &mut self,
        rows: &mut impl LinkingRows,
        owner: &mut impl LinkingLoadOwner,
    ) -> Result<()> {
        while let Some(row) = rows.next_template()? {
            let mut info = LinkingInfo {
                map: row.map,
                master: row.master,
                master_guid: 0,
                flags: row.flags,
                range: row.range,
            };
            let slave_exists = owner.template_exists(row.slave)?;
            let master_exists = owner.template_exists(row.master)?;
            let rejection = if !slave_exists {
                Some(LinkingRejection::SlaveTemplate)
            } else if !master_exists {
                Some(LinkingRejection::MasterTemplate)
            }
            // Source deliberately skips Map.dbc validation for map zero; cpp:190.
            else if row.map != 0 && !owner.map_exists(row.map)? {
                Some(LinkingRejection::Map)
            } else {
                Self::flags_valid(row.slave, info)
            };
            if let Some(reason) = rejection {
                self.reject(true, row.ordinal, row.slave, reason);
                continue;
            }
            if row.range == 0 && row.flags & (FOLLOW | PASSIVE) != 0 {
                match owner.query_unique_master(row.master, row.map)? {
                    LinkingMasterQuery::NoResult => {
                        self.reject(
                            true,
                            row.ordinal,
                            row.slave,
                            LinkingRejection::MissingUniqueMaster,
                        );
                        continue;
                    }
                    LinkingMasterQuery::Rows(guids) => {
                        require(
                            !guids.is_empty() && guids.len() <= 2,
                            "execution.linking.unique_query_shape",
                        )?;
                        if guids.len() > 1 {
                            self.reject(
                                true,
                                row.ordinal,
                                row.slave,
                                LinkingRejection::NonuniqueMaster,
                            );
                            continue;
                        }
                        info.master_guid = guids[0];
                    }
                }
            }
            self.entries.entry(row.slave).or_default().push(info);
            self.event_entries.insert(row.master);
        }
        while let Some(row) = rows.next_guid()? {
            let slave = owner.creature_data(row.slave)?;
            let master = owner.creature_data(row.master)?;
            let Some(slave) = slave else {
                self.reject(false, row.ordinal, row.slave, LinkingRejection::SlaveGuid);
                continue;
            };
            let Some(master) = master else {
                self.reject(false, row.ordinal, row.slave, LinkingRejection::MasterGuid);
                continue;
            };
            if slave.map != master.map {
                self.reject(
                    false,
                    row.ordinal,
                    row.slave,
                    LinkingRejection::DifferentMaps,
                );
                continue;
            }
            let info = LinkingInfo {
                map: u32::MAX,
                master: row.master,
                master_guid: row.master,
                flags: row.flags,
                range: 0,
            };
            if let Some(reason) = Self::flags_valid(row.slave, info) {
                self.reject(false, row.ordinal, row.slave, reason);
                continue;
            }
            self.guids.entry(row.slave).or_default().push(info);
            self.event_guids.insert(row.master);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkingHolderGroup {
    pub flags: u16,
    pub range: u16,
    pub slaves: Vec<(u32, SourceGuid)>,
    pub in_use: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkingHolderInventory {
    pub version: u32,
    pub map: MapIdentity,
    pub stopped: bool,
    pub entries: BTreeMap<u32, Vec<LinkingHolderGroup>>,
    pub guids: BTreeMap<u32, Vec<LinkingHolderGroup>>,
    pub masters: BTreeMap<u32, Vec<SourceGuid>>,
}
/// No destructor hook prunes these full-GUID records; cpp:328-398,528-550.
pub struct SourceCreatureLinkingHolder {
    map: MapIdentity,
    entries: BTreeMap<u32, Vec<LinkingHolderGroup>>,
    guids: BTreeMap<u32, Vec<LinkingHolderGroup>>,
    masters: BTreeMap<u32, Vec<SourceGuid>>,
    stopped: bool,
}
pub trait LinkingGateOwner {
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>>;
    fn wall_time(&mut self, map: MapIdentity, guid: u32) -> Result<ClockSample>;
    fn creature_pool(&mut self, guid: u32) -> Result<Option<u16>>;
    /// Executes the real instance predicate. None means there is no InstanceData.
    fn encounter_in_progress(&mut self, map: MapIdentity) -> Result<Option<bool>>;
}
enum GateRequest {
    Initial(LinkingInfo, [u32; 2]),
    Recursive(u32),
    Ready(u32, bool),
}
impl SourceCreatureLinkingHolder {
    pub fn new(map: MapIdentity) -> Result<Self> {
        require(
            map.creation_generation > 0,
            "execution.linking.map_generation",
        )?;
        Ok(Self {
            map,
            entries: BTreeMap::new(),
            guids: BTreeMap::new(),
            masters: BTreeMap::new(),
            stopped: false,
        })
    }
    pub fn inventory(&self) -> LinkingHolderInventory {
        LinkingHolderInventory {
            version: 1,
            map: self.map,
            stopped: self.stopped,
            entries: self.entries.clone(),
            guids: self.guids.clone(),
            masters: self.masters.clone(),
        }
    }
    pub fn map(&self) -> MapIdentity {
        self.map
    }
    fn object<'a>(
        &self,
        objects: &'a SourceObjects,
        handle: ObjectHandle,
    ) -> Result<&'a SourceObject> {
        let value = objects.get(handle)?;
        require(
            value.map == Some(self.map)
                && matches!(
                    value.kind,
                    SourceObjectKind::Creature | SourceObjectKind::Pet
                ),
            "execution.linking.object_map_kind",
        )?;
        Ok(value)
    }
    pub fn add_slave(
        &mut self,
        links: &SourceCreatureLinking,
        objects: &SourceObjects,
        handle: ObjectHandle,
        entry: u32,
    ) -> Result<()> {
        links.ensure_loaded()?;
        let object = self.object(objects, handle)?;
        let Some(info) = links.lookup(entry, object.database_guid, self.map.map) else {
            return Ok(());
        };
        let guid = object
            .guid
            .ok_or_else(|| refuse("execution.linking.guid_owner"))?;
        let holders = if info.map == u32::MAX {
            &mut self.guids
        } else {
            &mut self.entries
        };
        let groups = holders.entry(info.master).or_default();
        let index = groups
            .iter()
            .position(|g| g.flags == info.flags && (info.map == u32::MAX || g.range == info.range));
        let group = if let Some(index) = index {
            &mut groups[index]
        } else {
            groups.push(LinkingHolderGroup {
                flags: info.flags,
                range: info.range,
                slaves: Vec::new(),
                in_use: false,
            });
            groups.last_mut().unwrap()
        };
        group.slaves.push((object.database_guid, guid));
        Ok(())
    }
    pub fn add_master(
        &mut self,
        links: &SourceCreatureLinking,
        objects: &SourceObjects,
        handle: ObjectHandle,
        entry: u32,
    ) -> Result<()> {
        links.ensure_loaded()?;
        let object = self.object(objects, handle)?;
        if object.kind == SourceObjectKind::Pet || !links.is_master(entry) {
            return Ok(());
        }
        let guid = object
            .guid
            .ok_or_else(|| refuse("execution.linking.guid_owner"))?;
        let masters = self.masters.entry(entry).or_default();
        if !masters.contains(&guid) {
            masters.push(guid);
        }
        Ok(())
    }
    pub fn can_spawn_creature(
        &self,
        links: &SourceCreatureLinking,
        objects: &SourceObjects,
        state: &SourcePersistentState,
        owner: &mut impl LinkingGateOwner,
        handle: ObjectHandle,
        entry: u32,
    ) -> Result<bool> {
        links.ensure_loaded()?;
        let object = self.object(objects, handle)?;
        let Some(info) = links.lookup(entry, object.database_guid, self.map.map) else {
            return Ok(true);
        };
        let pose = object
            .respawn_pose
            .ok_or_else(|| refuse("execution.linking.respawn_pose_owner"))?;
        self.evaluate(
            links,
            objects,
            state,
            owner,
            GateRequest::Initial(info, [pose.0[0], pose.0[1]]),
        )
    }
    pub fn can_spawn_guid(
        &self,
        links: &SourceCreatureLinking,
        objects: &SourceObjects,
        state: &SourcePersistentState,
        owner: &mut impl LinkingGateOwner,
        guid: u32,
    ) -> Result<bool> {
        self.evaluate(links, objects, state, owner, GateRequest::Recursive(guid))
    }
    pub fn respawn_ready(
        &self,
        links: &SourceCreatureLinking,
        objects: &SourceObjects,
        state: &SourcePersistentState,
        owner: &mut impl LinkingGateOwner,
        guid: u32,
    ) -> Result<bool> {
        self.evaluate(
            links,
            objects,
            state,
            owner,
            GateRequest::Ready(guid, false),
        )
    }
    fn evaluate(
        &self,
        links: &SourceCreatureLinking,
        objects: &SourceObjects,
        state: &SourcePersistentState,
        owner: &mut impl LinkingGateOwner,
        mut request: GateRequest,
    ) -> Result<bool> {
        links.ensure_loaded()?;
        require(
            state.map_binding() == Some(self.map),
            "execution.linking.persistent_map_binding",
        )?;
        let mut frames = Vec::<(u32, bool)>::new();
        let mut recursive = BTreeSet::new();
        loop {
            let result = match request {
                GateRequest::Ready(guid, invert) => {
                    let deadline = state.respawn(GridSpawnKind::Creature, guid);
                    let ready = if deadline == 0 {
                        true
                    } else {
                        let wall = owner.wall_time(self.map, guid)?;
                        require(
                            wall.kind() == ClockKind::WallSeconds,
                            "execution.linking.wall_clock_kind",
                        )?;
                        deadline <= wall.value()
                    };
                    if ready {
                        require(
                            recursive.insert(guid),
                            "execution.linking.recursive_readiness_undefined",
                        )?;
                        frames.push((guid, invert));
                        request = GateRequest::Recursive(guid);
                        continue;
                    }
                    invert
                }
                GateRequest::Recursive(guid) => {
                    let Some(data) = owner.creature_data(guid)? else {
                        return Self::unwind(true, &mut frames, state, owner);
                    };
                    let Some(info) = links
                        .lookup(data.entry, guid, data.map)
                        .filter(|v| v.spawning_linked())
                    else {
                        return Self::unwind(true, &mut frames, state, owner);
                    };
                    request = GateRequest::Initial(info, data.respawn_xy);
                    continue;
                }
                GateRequest::Initial(info, xy) => {
                    if info.range == 0 {
                        if info.master_guid == 0 {
                            false
                        } else if info.flags & LINKING_BOSS_DEAD != 0 {
                            if owner.encounter_in_progress(self.map)? == Some(true) {
                                false
                            } else {
                                request = GateRequest::Ready(info.master_guid, false);
                                continue;
                            }
                        } else if info.flags & LINKING_BOSS_ALIVE != 0 {
                            request = GateRequest::Ready(info.master_guid, true);
                            continue;
                        } else {
                            true
                        }
                    } else {
                        let mut result = true;
                        if let Some(masters) = self.masters.get(&info.master) {
                            for &guid in masters {
                                let Some(handle) = objects.lookup_world_object(self.map, guid)
                                else {
                                    continue;
                                };
                                let master = objects.get(handle)?;
                                let pose = master.respawn_pose.ok_or_else(|| {
                                    refuse("execution.linking.master_respawn_pose_owner")
                                })?;
                                if linking_in_range(xy, [pose.0[0], pose.0[1]], info.range)? {
                                    result = if info.flags & LINKING_BOSS_DEAD != 0 {
                                        master.alive
                                    } else if info.flags & LINKING_BOSS_ALIVE != 0 {
                                        !master.alive
                                    } else {
                                        true
                                    };
                                    break;
                                }
                            }
                        }
                        result
                    }
                }
            };
            return Self::unwind(result, &mut frames, state, owner);
        }
    }
    fn unwind(
        mut value: bool,
        frames: &mut Vec<(u32, bool)>,
        state: &SourcePersistentState,
        owner: &mut impl LinkingGateOwner,
    ) -> Result<bool> {
        while let Some((guid, invert)) = frames.pop() {
            if value
                && owner.creature_pool(guid)?.is_some_and(|id| id != 0)
                && !state.pools().contains(PoolMember::Creature(guid))
            {
                value = false;
            }
            value ^= invert;
        }
        Ok(value)
    }
}
/// Source float operation order and strict boundary; cpp:647-659.
pub fn linking_in_range(slave: [u32; 2], master: [u32; 2], range: u16) -> Result<bool> {
    if range == 0 {
        return Ok(true);
    }
    let square = i32::from(range)
        .checked_mul(i32::from(range))
        .ok_or_else(|| refuse("execution.linking.range_square_undefined"))?;
    let dx =
        (Single::from_bits(u128::from(slave[0])) - Single::from_bits(u128::from(master[0]))).value;
    let dy =
        (Single::from_bits(u128::from(slave[1])) - Single::from_bits(u128::from(master[1]))).value;
    let distance = ((dx * dx).value + (dy * dy).value).value;
    Ok(distance < Single::from_i128(i128::from(square)).value)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LinkingPending {
    SealedLoaderOccurrences,
    EffectiveObjectMgrDefinitions,
    UniqueMasterSqlOccurrences,
    ReachedInstancePredicate,
    F5AiCombatMotion,
    MgMrLifecycle,
    F7Continuation,
}
pub const LINKING_PENDING: &[LinkingPending] = &[
    LinkingPending::SealedLoaderOccurrences,
    LinkingPending::EffectiveObjectMgrDefinitions,
    LinkingPending::UniqueMasterSqlOccurrences,
    LinkingPending::ReachedInstancePredicate,
    LinkingPending::F5AiCombatMotion,
    LinkingPending::MgMrLifecycle,
    LinkingPending::F7Continuation,
];
