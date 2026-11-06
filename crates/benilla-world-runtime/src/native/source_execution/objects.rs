// SPDX-License-Identifier: GPL-3.0-or-later
//! Construction, world membership and GUID lookup remain distinct lifetimes.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ObjectHandle {
    pub thread: u64,
    pub construction: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceObjectKind {
    Player,
    Creature,
    Pet,
    GameObject,
    Transport,
    DynamicObject,
    Corpse,
    Bones,
    Camera,
    Item,
    Container,
}
impl SourceObjectKind {
    // Pet/ordinary creature and bones/resurrectable corpse use different lists; GridDefines.h:68-69.
    pub(super) fn cell_slot(self) -> Option<usize> {
        Some(match self {
            Self::GameObject | Self::Transport => 0,
            Self::Creature => 1,
            Self::DynamicObject => 2,
            Self::Bones => 3,
            Self::Player => 4,
            Self::Pet => 5,
            Self::Corpse => 6,
            Self::Camera => 7,
            Self::Item | Self::Container => return None,
        })
    }
    fn tracks_current_cell(self) -> bool {
        matches!(
            self,
            Self::Creature | Self::Pet | Self::GameObject | Self::Transport
        )
    }
    fn unit(self) -> bool {
        matches!(self, Self::Creature | Self::Pet | Self::Player)
    }
    fn lookup_kind(self) -> Option<LiveLookupKind> {
        Some(match self {
            Self::Creature => LiveLookupKind::Creature,
            Self::Pet => LiveLookupKind::Pet,
            Self::GameObject | Self::Transport => LiveLookupKind::GameObject,
            Self::DynamicObject => LiveLookupKind::DynamicObject,
            Self::Corpse | Self::Bones => LiveLookupKind::Corpse,
            Self::Player => LiveLookupKind::Player,
            _ => return None,
        })
    }
    fn database_kind(self) -> Option<DatabaseLookupKind> {
        match self {
            Self::Creature => Some(DatabaseLookupKind::Creature),
            Self::GameObject | Self::Transport => Some(DatabaseLookupKind::GameObject),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiveLookupKind {
    Creature,
    Pet,
    GameObject,
    DynamicObject,
    Corpse,
    Player,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DatabaseLookupKind {
    Creature,
    GameObject,
}
#[derive(Clone, Debug)]
pub struct SourceObject {
    pub handle: ObjectHandle,
    pub kind: SourceObjectKind,
    pub guid: Option<SourceGuid>,
    pub database_guid: u32,
    pub map: Option<MapIdentity>,
    pub current_cell: Option<CellIdentity>,
    pub in_world: bool,
    pub alive: bool,
    pub life_generation: u64,
    pub active: bool,
    pub pose: Option<SourcePose>,
    pub respawn_pose: Option<SourcePose>,
    pub database_spawn_data: bool,
    pub(super) source_allocation: Option<u64>,
    allocation_base: Option<u64>,
    player_map_linked: bool,
    global_player_registered: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseObservation {
    NoLiveObject,
    NoRegistration,
    Matched(ObjectHandle),
    DifferentCell(ObjectHandle),
    DifferentIdentity(ObjectHandle),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ObjectRegistrationPending {
    SealedAllocatorStarts,
    FullGuidObservation,
    ConstructionCausality,
    RegistrationAndCellState,
    GenerationAssociations,
    CameraAndViewpointCallbacks,
}
#[derive(Default)]
pub struct SourceObjects {
    next: BTreeMap<u64, u64>,
    objects: BTreeMap<ObjectHandle, SourceObject>,
    lookup: BTreeMap<(Option<MapIdentity>, LiveLookupKind, SourceGuid), ObjectHandle>,
    database: BTreeMap<(MapIdentity, DatabaseLookupKind, u32), Vec<ObjectHandle>>,
    allocations: BTreeMap<u64, (u64, ObjectHandle)>,
    players: BTreeMap<MapIdentity, std::collections::VecDeque<ObjectHandle>>,
}
impl SourceObjects {
    // A construction identity exists before _Create assigns any GUID; Object.cpp:48-59,115-125.
    pub fn construct(&mut self, thread: u64, kind: SourceObjectKind) -> Result<ObjectHandle> {
        require(thread > 0, "execution.object_thread")?;
        let next = self.next.entry(thread).or_insert(1);
        let handle = ObjectHandle {
            thread,
            construction: *next,
        };
        *next = next
            .checked_add(1)
            .ok_or_else(|| refuse("execution.construction_generation_exhausted"))?;
        self.objects.insert(
            handle,
            SourceObject {
                handle,
                kind,
                guid: None,
                database_guid: 0,
                map: None,
                current_cell: None,
                in_world: false,
                alive: true,
                life_generation: 1,
                active: kind == SourceObjectKind::Player,
                pose: None,
                respawn_pose: None,
                database_spawn_data: false,
                source_allocation: None,
                allocation_base: None,
                player_map_linked: false,
                global_player_registered: false,
            },
        );
        Ok(handle)
    }
    pub fn get(&self, handle: ObjectHandle) -> Result<&SourceObject> {
        self.objects
            .get(&handle)
            .ok_or_else(|| refuse("execution.stale_object_handle"))
    }
    pub(super) fn get_mut(&mut self, handle: ObjectHandle) -> Result<&mut SourceObject> {
        self.objects
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.stale_object_handle"))
    }
    pub fn pose(&self, handle: ObjectHandle) -> Result<SourcePose> {
        self.get(handle)?
            .pose
            .ok_or_else(|| refuse("execution.active.position_owner"))
    }
    pub fn set_pose(&mut self, handle: ObjectHandle, pose: SourcePose) -> Result<()> {
        self.get_mut(handle)?.pose = Some(pose);
        Ok(())
    }
    pub fn set_respawn_definition(
        &mut self,
        handle: ObjectHandle,
        pose: SourcePose,
        database_spawn_data: bool,
    ) -> Result<()> {
        let object = self.get_mut(handle)?;
        object.respawn_pose = Some(pose);
        object.database_spawn_data = database_spawn_data;
        Ok(())
    }
    // Source pointer order uses a live allocation extent, never a GUID/handle sort; EntitiesMgr.h:43.
    pub fn bind_source_allocation(
        &mut self,
        handle: ObjectHandle,
        base: u64,
        bytes: u64,
        offset: u64,
    ) -> Result<()> {
        require(
            self.get(handle)?.source_allocation.is_none()
                && base != 0
                && bytes != 0
                && offset < bytes,
            "execution.active.allocation_binding",
        )?;
        let end = base
            .checked_add(bytes)
            .ok_or_else(|| refuse("execution.active.allocation_extent"))?;
        require(
            self.allocations
                .range(..=base)
                .next_back()
                .is_none_or(|(_, (end, _))| *end <= base)
                && self
                    .allocations
                    .range(base..)
                    .next()
                    .is_none_or(|(&next, _)| end <= next),
            "execution.active.allocation_overlap",
        )?;
        self.allocations.insert(base, (end, handle));
        let object = self.get_mut(handle)?;
        object.source_allocation = Some(base + offset);
        object.allocation_base = Some(base);
        Ok(())
    }
    // Map links the player before entry-grid callbacks, independently of world membership; Map.cpp:446-453.
    pub fn link_map_player(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get(handle)?;
        require(
            object.kind == SourceObjectKind::Player,
            "execution.active.not_player",
        )?;
        let map = object
            .map
            .ok_or_else(|| refuse("execution.active.player_map"))?;
        if !object.player_map_linked {
            self.players.entry(map).or_default().push_front(handle);
            self.get_mut(handle)?.player_map_linked = true;
        }
        Ok(())
    }
    pub fn unlink_map_player(&mut self, handle: ObjectHandle) -> Result<()> {
        let map = self
            .get(handle)?
            .map
            .ok_or_else(|| refuse("execution.active.player_map"))?;
        if let Some(players) = self.players.get_mut(&map) {
            players.retain(|value| *value != handle);
        }
        self.get_mut(handle)?.player_map_linked = false;
        Ok(())
    }
    pub fn map_players(&self, map: MapIdentity) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.players.get(&map).into_iter().flatten().copied()
    }
    pub fn assign_guid(
        &mut self,
        handle: ObjectHandle,
        database_guid: u32,
        guid: SourceGuid,
    ) -> Result<()> {
        let object = self.get_mut(handle)?;
        require(
            !object.in_world
                && !object.global_player_registered
                && object.kind != SourceObjectKind::Camera,
            "execution.object_guid_assignment",
        )?;
        object.guid = Some(guid);
        object.database_guid = database_guid;
        Ok(())
    }
    pub fn bind_map(
        &mut self,
        handle: ObjectHandle,
        map: MapIdentity,
        cells: &SourceCells,
    ) -> Result<()> {
        require(
            map == cells.map()
                && map.creation_generation > 0
                && cells.registered_cell(handle).is_none(),
            "execution.object_map_generation",
        )?;
        let object = self.get_mut(handle)?;
        require(!object.in_world, "execution.object_in_world_map_change")?;
        object.map = Some(map);
        Ok(())
    }
    pub fn link_cell(
        &mut self,
        handle: ObjectHandle,
        cell: CellIdentity,
        cells: &mut SourceCells,
    ) -> Result<()> {
        let object = self.get(handle)?;
        require(
            object.map == Some(cell.grid.map) && cells.map() == cell.grid.map,
            "execution.object_cell_map",
        )?;
        let kind = object.kind;
        cells.link(handle, kind, cell)?;
        if kind.tracks_current_cell() {
            self.get_mut(handle)?.current_cell = Some(cell);
        }
        Ok(())
    }
    pub fn unlink_cell(&mut self, handle: ObjectHandle, cells: &mut SourceCells) -> Result<()> {
        let object = self.get(handle)?;
        require(object.map == Some(cells.map()), "execution.object_cell_map")?;
        cells.unlink(handle);
        Ok(())
    }
    fn lookup_key(
        object: &SourceObject,
        kind: LiveLookupKind,
    ) -> Result<(Option<MapIdentity>, LiveLookupKind, SourceGuid)> {
        let guid = object
            .guid
            .ok_or_else(|| refuse("execution.object_guid_absent"))?;
        let map = if matches!(kind, LiveLookupKind::Player | LiveLookupKind::Corpse) {
            None
        } else {
            Some(
                object
                    .map
                    .ok_or_else(|| refuse("execution.object_map_absent"))?,
            )
        };
        Ok((map, kind, guid))
    }
    fn insert_lookup(&mut self, handle: ObjectHandle, kind: LiveLookupKind) -> Result<()> {
        let key = Self::lookup_key(self.get(handle)?, kind)?;
        // A duplicate key may identify this same pointer, never a different object; TypeContainer.h:94-103.
        require(
            self.lookup
                .get(&key)
                .is_none_or(|existing| *existing == handle),
            "execution.duplicate_live_guid",
        )?;
        self.lookup.insert(key, handle);
        Ok(())
    }
    // Player accessor registration belongs to Login, not Player::AddToWorld; CharacterHandler.cpp:822-832.
    pub fn register_player_lookup(&mut self, handle: ObjectHandle) -> Result<()> {
        require(
            self.get(handle)?.kind == SourceObjectKind::Player,
            "execution.player_lookup_kind",
        )?;
        self.insert_lookup(handle, LiveLookupKind::Player)?;
        self.get_mut(handle)?.global_player_registered = true;
        Ok(())
    }
    pub fn remove_player_lookup(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get(handle)?;
        require(
            object.kind == SourceObjectKind::Player,
            "execution.player_lookup_kind",
        )?;
        let key = Self::lookup_key(object, LiveLookupKind::Player)?;
        self.lookup.remove(&key);
        self.get_mut(handle)?.global_player_registered = false;
        Ok(())
    }
    // Typed/DB lookup precedes base world membership; Creature.cpp:173-202, GameObject.cpp:115-131.
    pub fn register_world_lookup(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get(handle)?;
        if object.in_world {
            return Ok(());
        }
        let kind = object.kind;
        let database_key = object
            .map
            .zip(kind.database_kind())
            .filter(|_| object.database_guid != 0)
            .map(|(map, kind)| (map, kind, object.database_guid));
        require(
            kind != SourceObjectKind::Camera,
            "execution.camera_not_world_object",
        )?;
        if let Some(lookup_kind) = kind
            .lookup_kind()
            .filter(|kind| *kind != LiveLookupKind::Player)
        {
            self.insert_lookup(handle, lookup_kind)?;
        }
        if let Some(key) = database_key {
            let values = self.database.entry(key).or_default();
            if !values.contains(&handle) {
                values.push(handle);
            }
        }
        Ok(())
    }
    // Base Object::AddToWorld follows the entity-specific work; GameObject.cpp:125-131, Creature.cpp:200-202.
    pub fn mark_in_world(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get(handle)?;
        require(
            object.kind != SourceObjectKind::Camera,
            "execution.camera_not_world_object",
        )?;
        if let Some(kind) = object
            .kind
            .lookup_kind()
            .filter(|kind| *kind != LiveLookupKind::Player)
        {
            require(
                self.lookup.get(&Self::lookup_key(object, kind)?) == Some(&handle),
                "execution.lookup_before_world",
            )?;
        }
        self.get_mut(handle)?.in_world = true;
        Ok(())
    }
    // Contiguous callers use both phases without an intervening entity hook; Creature.cpp:173-202, Pet.cpp:70-76.
    pub fn add_to_world(&mut self, handle: ObjectHandle) -> Result<()> {
        self.register_world_lookup(handle)?;
        self.mark_in_world(handle)
    }
    // Removing from world does not unlink a grid reference or destroy the C++ instance; Map.cpp:1107-1113.
    pub fn remove_from_world(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get(handle)?;
        if !object.in_world {
            return Ok(());
        }
        let kind = object.kind;
        let database_key = object
            .map
            .zip(kind.database_kind())
            .map(|(map, kind)| (map, kind, object.database_guid));
        if let Some(lookup_kind) = kind
            .lookup_kind()
            .filter(|kind| *kind != LiveLookupKind::Player)
        {
            let key = Self::lookup_key(object, lookup_kind)?;
            self.lookup.remove(&key);
        }
        if let Some(key) = database_key {
            if let Some(objects) = self.database.get_mut(&key) {
                objects.retain(|object| *object != handle);
            }
        }
        self.get_mut(handle)?.in_world = false;
        Ok(())
    }
    pub fn lookup(
        &self,
        map: MapIdentity,
        kind: LiveLookupKind,
        guid: SourceGuid,
    ) -> Option<ObjectHandle> {
        let owner = if matches!(kind, LiveLookupKind::Player | LiveLookupKind::Corpse) {
            None
        } else {
            Some(map)
        };
        self.lookup
            .get(&(owner, kind, guid))
            .copied()
            .filter(|handle| {
                self.objects
                    .get(handle)
                    .is_some_and(|object| object.map == Some(map) && object.in_world)
            })
    }
    // DB lookup preserves insertion order and prioritizes an alive creature; Map.cpp:2426-2452,2515-2523.
    pub fn lookup_database(
        &self,
        map: MapIdentity,
        kind: DatabaseLookupKind,
        guid: u32,
    ) -> Option<ObjectHandle> {
        let objects = self.database.get(&(map, kind, guid))?;
        if kind == DatabaseLookupKind::Creature {
            if let Some(handle) = objects
                .iter()
                .find(|handle| self.objects.get(handle).is_some_and(|object| object.alive))
            {
                return Some(*handle);
            }
        }
        objects.first().copied()
    }
    pub fn candidate_allowed(
        &self,
        map: MapIdentity,
        kind: DatabaseLookupKind,
        database_guid: u32,
    ) -> bool {
        self.lookup_database(map, kind, database_guid)
            .is_none_or(|handle| {
                kind == DatabaseLookupKind::Creature && !self.objects[&handle].alive
            })
    }
    pub fn mark_dead(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get_mut(handle)?;
        require(object.kind.unit(), "execution.object_not_unit")?;
        object.alive = false;
        Ok(())
    }
    // Track a new life without reconstructing or reassigning GUID; Creature.cpp:717-731.
    pub fn legacy_respawn(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get_mut(handle)?;
        require(
            object.kind.unit() && !object.alive,
            "execution.object_respawn_state",
        )?;
        object.life_generation = object
            .life_generation
            .checked_add(1)
            .ok_or_else(|| refuse("execution.life_generation_exhausted"))?;
        object.alive = true;
        Ok(())
    }
    pub fn relocate_creature(
        &mut self,
        handle: ObjectHandle,
        target: CellPair,
        cells: &mut SourceCells,
        load_at_enter: impl FnOnce(&mut Self, &mut SourceCells, CellPair) -> Result<()>,
    ) -> Result<CellRelocation> {
        let object = self.get(handle)?;
        require(object.map == Some(cells.map()), "execution.object_cell_map")?;
        let kind = object.kind;
        let active = object.active;
        let result = cells.relocate_creature(handle, kind, target, active, |cells, pair| {
            load_at_enter(self, cells, pair)
        })?;
        if let CellRelocation::Moved { new, .. } = result {
            self.get_mut(handle)?.current_cell = Some(new);
        }
        Ok(result)
    }
    // Object destruction asserts not in world, then GridReference destruction unlinks; Object.cpp:61-68, GridReference.h:58-61.
    pub fn destroy(
        &mut self,
        handle: ObjectHandle,
        cells: &mut SourceCells,
    ) -> Result<SourceObject> {
        let object = self.get(handle)?;
        require(
            !object.in_world
                && !object.global_player_registered
                && !object.player_map_linked
                && !object
                    .source_allocation
                    .is_some_and(|pointer| cells.active().contains_source_pointer(pointer)),
            "execution.destroy_registered_object",
        )?;
        require(
            object.map.is_none_or(|map| map == cells.map()),
            "execution.object_cell_map",
        )?;
        if let Some(base) = object.allocation_base {
            self.allocations.remove(&base);
        }
        cells.unlink(handle);
        Ok(self.objects.remove(&handle).unwrap())
    }
    // A pose observation can compare with registration but never create it; creature_pose_cells.rs:174-183.
    pub fn compare_pose_observation(
        &self,
        map: MapIdentity,
        observation: &crate::native::CreaturePoseCell,
        cells: &SourceCells,
    ) -> PoseObservation {
        let identity = observation.identity();
        let Some(handle) = self.lookup(
            map,
            LiveLookupKind::Creature,
            SourceGuid::from_raw(identity.guid()),
        ) else {
            return PoseObservation::NoLiveObject;
        };
        let object = &self.objects[&handle];
        if object.database_guid != identity.database_guid()
            || object
                .guid
                .is_none_or(|guid| guid.entry() != identity.entry())
        {
            return PoseObservation::DifferentIdentity(handle);
        }
        let Some(registered) = cells.registered_cell(handle) else {
            return PoseObservation::NoRegistration;
        };
        let pose = observation.cell();
        if (registered.x, registered.y) == (pose.x(), pose.y()) {
            PoseObservation::Matched(handle)
        } else {
            PoseObservation::DifferentCell(handle)
        }
    }
}
