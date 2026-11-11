// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Mutable source state. Inventories are diagnostic, not durable checkpoints.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentStateIdentity {
    pub map: u32,
    pub instance: u32,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentMapEntry {
    pub id: u32,
    pub map_type: u32,
}
impl PersistentMapEntry {
    // DBCStructure.h:593-597: unknown types follow the non-instanceable branch.
    pub fn instanceable(self) -> bool {
        matches!(self.map_type, 1..=3)
    }
    pub fn dungeon(self) -> bool {
        matches!(self.map_type, 1 | 2)
    }
    pub fn battleground(self) -> bool {
        self.map_type == 3
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PoolMember {
    Creature(u32),
    GameObject(u32),
    Pool(u32),
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PersistentPoolData {
    creatures: BTreeSet<u32>,
    gameobjects: BTreeSet<u32>,
    pools: BTreeMap<u32, u32>,
    initialized: bool,
}
impl PersistentPoolData {
    /// Join retained selections, including unloaded leaves and zero child counts.
    pub(super) fn validate_manager_owners(&self, pools: &PoolInventory) -> Result<()> {
        require(
            self.creatures
                .iter()
                .all(|guid| pools.membership[2].contains_key(guid))
                && self
                    .gameobjects
                    .iter()
                    .all(|guid| pools.membership[1].contains_key(guid))
                && self
                    .pools
                    .keys()
                    .all(|pool| (*pool as usize) < pools.templates.len()),
            "execution.managers.selected_pool_owner",
        )
    }
    pub fn initialized(&self) -> bool {
        self.initialized
    }
    // InitPools sets this before its real pool/event callbacks; MapPersistentStateMgr.cpp:198-206.
    pub fn mark_initialized(&mut self) {
        self.initialized = true;
    }
    pub fn contains(&self, member: PoolMember) -> bool {
        match member {
            PoolMember::Creature(id) => self.creatures.contains(&id),
            PoolMember::GameObject(id) => self.gameobjects.contains(&id),
            PoolMember::Pool(id) => self.pools.contains_key(&id),
        }
    }
    pub fn count(&self, pool: u32) -> u32 {
        self.pools.get(&pool).copied().unwrap_or(0)
    }
    // Duplicate leaves still increment; child insertion overwrites its count; PoolManager.cpp:62-81.
    pub fn add(&mut self, member: PoolMember, pool: u32) {
        match member {
            PoolMember::Creature(id) => {
                self.creatures.insert(id);
            }
            PoolMember::GameObject(id) => {
                self.gameobjects.insert(id);
            }
            PoolMember::Pool(id) => {
                self.pools.insert(id, 0);
            }
        }
        let count = self.pools.entry(pool).or_default();
        *count = count.wrapping_add(1);
    }
    // Absent removals create a retained zero parent; PoolManager.cpp:83-107.
    pub fn remove(&mut self, member: PoolMember, pool: u32) {
        match member {
            PoolMember::Creature(id) => {
                self.creatures.remove(&id);
            }
            PoolMember::GameObject(id) => {
                self.gameobjects.remove(&id);
            }
            PoolMember::Pool(id) => {
                self.pools.remove(&id);
            }
        }
        let count = self.pools.entry(pool).or_default();
        if *count > 0 {
            *count -= 1;
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PersistentCellGuids {
    pub creatures: BTreeSet<u32>,
    pub gameobjects: BTreeSet<u32>,
}
pub fn persistent_cell_key(cell: CellPair) -> u32 {
    // MapPersistentStateMgr.cpp:168-169, not x * 1024 + y.
    u32::from(cell.y()) * 1024 + u32::from(cell.x())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PersistentInventory {
    pub version: u32,
    pub identity: PersistentStateIdentity,
    pub entry: PersistentMapEntry,
    pub used_by_map: Option<MapIdentity>,
    pub reset_time: i64,
    pub can_reset: bool,
    pub completed_encounters: u32,
    pub player_binds: Vec<u64>,
    pub group_binds: Vec<u64>,
    pub creature_deadlines: BTreeMap<u32, i64>,
    pub gameobject_deadlines: BTreeMap<u32, i64>,
    pub cells: BTreeMap<u32, PersistentCellGuids>,
    pub pools: PersistentPoolData,
}
impl PersistentInventory {
    /// Structural validation only. F7 must additionally join foreign owner lifetimes.
    pub fn validate(&self) -> Result<()> {
        require(self.version == 1, "execution.persistent.inventory_version")?;
        require(
            self.identity.generation > 0
                && self.identity.map == self.entry.id
                && self.entry.id <= u16::MAX as u32
                && (self.entry.instanceable() || self.identity.instance == 0),
            "execution.persistent.inventory_identity",
        )?;
        require(
            self.used_by_map.is_none_or(|map| {
                map.map == self.identity.map
                    && map.instance == self.identity.instance
                    && map.creation_generation > 0
            }),
            "execution.persistent.inventory_map",
        )?;
        require(
            self.cells.keys().all(|key| *key < 1024 * 1024),
            "execution.persistent.inventory_cell",
        )?;
        require(
            (self.entry.dungeon() || (self.player_binds.is_empty() && self.group_binds.is_empty()))
                && self
                    .player_binds
                    .iter()
                    .chain(&self.group_binds)
                    .all(|id| *id > 0),
            "execution.persistent.inventory_binds",
        )
    }
}
pub struct SourcePersistentState {
    pub(super) identity: PersistentStateIdentity,
    pub(super) entry: PersistentMapEntry,
    pub(super) used_by_map: Option<MapIdentity>,
    pub(super) reset_time: i64,
    pub(super) can_reset: bool,
    pub(super) completed_encounters: u32,
    pub(super) player_binds: Vec<u64>,
    pub(super) group_binds: Vec<u64>,
    creature_deadlines: HashMap<u32, i64>,
    gameobject_deadlines: HashMap<u32, i64>,
    cells: HashMap<u32, PersistentCellGuids>,
    pools: PersistentPoolData,
}
impl SourcePersistentState {
    pub(super) fn new(
        identity: PersistentStateIdentity,
        entry: PersistentMapEntry,
        reset_time: i64,
        can_reset: bool,
        completed_encounters: u32,
    ) -> Self {
        Self {
            identity,
            entry,
            used_by_map: None,
            reset_time,
            can_reset,
            completed_encounters,
            player_binds: Vec::new(),
            group_binds: Vec::new(),
            creature_deadlines: HashMap::new(),
            gameobject_deadlines: HashMap::new(),
            cells: HashMap::new(),
            pools: PersistentPoolData::default(),
        }
    }
    pub fn identity(&self) -> PersistentStateIdentity {
        self.identity
    }
    pub fn map_binding(&self) -> Option<MapIdentity> {
        self.used_by_map
    }
    pub fn pools(&self) -> &PersistentPoolData {
        &self.pools
    }
    pub fn pools_mut(&mut self) -> &mut PersistentPoolData {
        &mut self.pools
    }
    pub fn respawn(&self, kind: GridSpawnKind, guid: u32) -> i64 {
        match kind {
            GridSpawnKind::Creature => &self.creature_deadlines,
            GridSpawnKind::GameObject => &self.gameobject_deadlines,
        }
        .get(&guid)
        .copied()
        .unwrap_or(0)
    }
    pub(super) fn set_respawn(
        &mut self,
        kind: GridSpawnKind,
        guid: u32,
        time: i64,
        world: i64,
    ) -> i64 {
        let deadlines = match kind {
            GridSpawnKind::Creature => &mut self.creature_deadlines,
            GridSpawnKind::GameObject => &mut self.gameobject_deadlines,
        };
        if time > world {
            deadlines.insert(guid, time);
            time
        } else {
            deadlines.remove(&guid);
            0
        }
    }
    pub fn cell_guids(&mut self, cell: CellPair) -> &PersistentCellGuids {
        self.cells.entry(persistent_cell_key(cell)).or_default()
    }
    pub(super) fn overlay(
        &mut self,
        cell: CellPair,
        kind: GridSpawnKind,
        guid: u32,
        insert: bool,
    ) -> bool {
        let cell = self.cells.entry(persistent_cell_key(cell)).or_default();
        let set = match kind {
            GridSpawnKind::Creature => &mut cell.creatures,
            GridSpawnKind::GameObject => &mut cell.gameobjects,
        };
        if insert {
            set.insert(guid)
        } else {
            set.remove(&guid)
        }
    }
    // World states never unload; BG ignores deadlines; dungeon checks both bind lists; .cpp:209-216,246-249,338-342.
    pub(super) fn unloadable(&self) -> bool {
        self.used_by_map.is_none()
            && if self.entry.dungeon() {
                self.player_binds.is_empty()
                    && self.group_binds.is_empty()
                    && self.creature_deadlines.is_empty()
                    && self.gameobject_deadlines.is_empty()
            } else {
                self.entry.battleground()
            }
    }
    /// Canonical enumeration only. Source unordered traversal requires its own legal-history owner.
    pub fn inventory(&self) -> PersistentInventory {
        PersistentInventory {
            version: 1,
            identity: self.identity,
            entry: self.entry,
            used_by_map: self.used_by_map,
            reset_time: self.reset_time,
            can_reset: self.can_reset,
            completed_encounters: self.completed_encounters,
            player_binds: self.player_binds.clone(),
            group_binds: self.group_binds.clone(),
            creature_deadlines: self
                .creature_deadlines
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
            gameobject_deadlines: self
                .gameobject_deadlines
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
            cells: self.cells.iter().map(|(&k, v)| (k, v.clone())).collect(),
            pools: self.pools.clone(),
        }
    }
}
