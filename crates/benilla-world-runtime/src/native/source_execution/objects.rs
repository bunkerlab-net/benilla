// SPDX-License-Identifier: GPL-3.0-or-later
//! Construction, world membership and GUID lookup remain distinct lifetimes.
use super::active::SourcePointerSet;
use super::*;
use benilla_sim::resident_map::CellPair;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

const OBJECT_END: usize = 0x06; // UpdateFields.h:31.
pub(super) const OBJECT_FIELD_GUID: usize = 0x00; // UpdateFields.h:26.
pub(super) const OBJECT_FIELD_ENTRY: usize = 0x03; // UpdateFields.h:28.
pub(super) const OBJECT_FIELD_SCALE_X: usize = 0x04; // UpdateFields.h:29.
pub(super) const UNIT_FIELD_TARGET: usize = OBJECT_END + 0x0a; // UpdateFields.h:73.
pub(super) const UNIT_FIELD_HEALTH: usize = OBJECT_END + 0x10; // UpdateFields.h:76.
pub(super) const UNIT_FIELD_POWER2: usize = OBJECT_END + 0x12; // UpdateFields.h:78.
pub(super) const UNIT_FIELD_FACTIONTEMPLATE: usize = OBJECT_END + 0x1d; // UpdateFields.h:89.
pub(super) const UNIT_FIELD_BYTES_0: usize = OBJECT_END + 0x1e; // UpdateFields.h:90.
pub(super) const UNIT_FIELD_FLAGS: usize = OBJECT_END + 0x28; // UpdateFields.h:100.
pub(super) const UNIT_FIELD_AURA: usize = OBJECT_END + 0x29; // UpdateFields.h:101.
pub(super) const UNIT_FIELD_AURAFLAGS: usize = OBJECT_END + 0x59; // UpdateFields.h:103.
pub(super) const UNIT_FIELD_AURALEVELS: usize = OBJECT_END + 0x5f; // UpdateFields.h:109.
pub(super) const UNIT_FIELD_AURAAPPLICATIONS: usize = OBJECT_END + 0x6b; // UpdateFields.h:111.
pub(super) const UNIT_FIELD_MOUNTDISPLAYID: usize = OBJECT_END + 0x7f; // UpdateFields.h:121.
pub(super) const UNIT_FIELD_BYTES_1: usize = OBJECT_END + 0x84; // UpdateFields.h:126.
pub(super) const UNIT_NPC_FLAGS: usize = OBJECT_END + 0x8d; // UpdateFields.h:135.
pub(super) const UNIT_NPC_EMOTESTATE: usize = OBJECT_END + 0x8e; // UpdateFields.h:136.
pub(super) const UNIT_FIELD_STAT1: usize = OBJECT_END + 0x91; // UpdateFields.h:139.
pub(super) const UNIT_FIELD_RESISTANCES: usize = OBJECT_END + 0x95; // UpdateFields.h:143.
pub(super) const UNIT_FIELD_BYTES_2: usize = OBJECT_END + 0x9e; // UpdateFields.h:152.
pub(super) const GAMEOBJECT_DISPLAYID: usize = OBJECT_END + 0x02; // UpdateFields.h:272.
pub(super) const GAMEOBJECT_FLAGS: usize = OBJECT_END + 0x03; // UpdateFields.h:273.
pub(super) const GAMEOBJECT_STATE: usize = OBJECT_END + 0x08; // UpdateFields.h:275.
pub(super) const GAMEOBJECT_DYN_FLAGS: usize = OBJECT_END + 0x0d; // UpdateFields.h:280.
pub(super) const GAMEOBJECT_FACTION: usize = OBJECT_END + 0x0e; // UpdateFields.h:281.
pub(super) const GAMEOBJECT_TYPE_ID: usize = OBJECT_END + 0x0f; // UpdateFields.h:282.
pub(super) const GAMEOBJECT_ANIMPROGRESS: usize = OBJECT_END + 0x12; // UpdateFields.h:285.
const UNIT_END: usize = OBJECT_END + 0xb6; // UpdateFields.h:176.
const PLAYER_END: usize = UNIT_END + 0x446; // UpdateFields.h:266.
const GAMEOBJECT_END: usize = OBJECT_END + 0x14; // UpdateFields.h:287.
const UPDATE_WORD_BITS: usize = u64::BITS as usize;

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
    pub notify_on_event: bool,
    pub pose: Option<SourcePose>,
    pub respawn_pose: Option<SourcePose>,
    pub database_spawn_data: bool,
    // Cache UpdateZone callbacks separately from terrain GetZoneId; Player.cpp:505,6854.
    pub zone_update_id: u32,
    pub is_new_object: bool,
    pub(super) visibility: Option<WorldVisibility>,
    // Corpse GridReference can differ from SetMap; ObjectAccessor.cpp:203-221.
    pub(super) corpse_grid_reference: Option<CellIdentity>,
    pub(super) source_allocation: Option<u64>,
    allocation_base: Option<u64>,
    player_map_linked: bool,
    global_player_registered: bool,
    // Re-entrant setters mark through a shared object (Object.cpp:1107-1117).
    changed_fields: SourceUpdateMask,
    object_updated: Cell<bool>,
}

/// Object.cpp:105-112 starts with one clear changed bit per update field word.
#[derive(Clone, Debug)]
enum SourceUpdateMask {
    Unit([Cell<u64>; UNIT_END.div_ceil(UPDATE_WORD_BITS)]),
    Player(Box<[Cell<u64>; PLAYER_END.div_ceil(UPDATE_WORD_BITS)]>),
    GameObject([Cell<u64>; GAMEOBJECT_END.div_ceil(UPDATE_WORD_BITS)]),
    Unbound,
}

impl SourceUpdateMask {
    fn new(kind: SourceObjectKind) -> Self {
        match kind {
            // Creature.cpp:153; Pet.cpp:51 uses the Creature constructor.
            SourceObjectKind::Creature | SourceObjectKind::Pet => {
                Self::Unit([const { Cell::new(0) }; UNIT_END.div_ceil(UPDATE_WORD_BITS)])
            }
            // Player.cpp:484 uses PLAYER_END.
            SourceObjectKind::Player => Self::Player(Box::new(
                [const { Cell::new(0) }; PLAYER_END.div_ceil(UPDATE_WORD_BITS)],
            )),
            // GameObject.cpp:74; Transports.h:30,71,83.
            SourceObjectKind::GameObject | SourceObjectKind::Transport => Self::GameObject(
                [const { Cell::new(0) }; GAMEOBJECT_END.div_ceil(UPDATE_WORD_BITS)],
            ),
            _ => Self::Unbound,
        }
    }

    fn count(&self) -> Result<usize> {
        match self {
            Self::Unit(_) => Ok(UNIT_END),
            Self::Player(_) => Ok(PLAYER_END),
            Self::GameObject(_) => Ok(GAMEOBJECT_END),
            Self::Unbound => Err(refuse(
                "execution.object.Object_105.update_values_count_owner",
            )),
        }
    }

    fn words(&self) -> &[Cell<u64>] {
        match self {
            Self::Unit(bits) => bits,
            Self::Player(bits) => bits.as_slice(),
            Self::GameObject(bits) => bits,
            Self::Unbound => &[],
        }
    }
}

impl SourceUnitFields {
    /// Unit.h:1363 sets the faction update field before later lookups can read it.
    pub(super) fn set_faction_template(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        faction: u32,
    ) -> Result<()> {
        if objects.set_update_value(unit, UNIT_FIELD_FACTIONTEMPLATE, &mut self.faction, faction)? {
            // Unit.cpp:6228-6238 reads the current faction template.
            self.neutral_to_all = None;
        }
        Ok(())
    }
}

impl SourceObject {
    pub fn field_changed(&self, field: usize) -> Result<bool> {
        require(
            field < self.changed_fields.count()?,
            "execution.object.Object_849.update_field_index_assert",
        )?;
        Ok(self.changed_fields.words()[field / UPDATE_WORD_BITS].get()
            & (1 << (field % UPDATE_WORD_BITS))
            != 0)
    }
}

#[cfg(test)]
#[path = "update_marks_tests.rs"]
mod update_marks_tests;

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
    client_updates: RefCell<BTreeMap<MapIdentity, BTreeSet<ObjectHandle>>>,
    event_notified: BTreeMap<MapIdentity, SourcePointerSet>,
}
impl SourceObjects {
    pub fn construction_count(&self, thread: u64) -> u64 {
        self.next.get(&thread).map_or(0, |&next| next - 1)
    }
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
                notify_on_event: false,
                pose: None,
                respawn_pose: None,
                database_spawn_data: false,
                zone_update_id: 0,
                is_new_object: false, // Object.cpp:48.
                visibility: (!matches!(
                    kind,
                    SourceObjectKind::Camera | SourceObjectKind::Item | SourceObjectKind::Container
                ))
                .then(WorldVisibility::default),
                corpse_grid_reference: None,
                source_allocation: None,
                allocation_base: None,
                player_map_linked: false,
                global_player_registered: false,
                changed_fields: SourceUpdateMask::new(kind),
                object_updated: Cell::new(false),
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
    /// Object.cpp:847-856,884-900,970-995. Compare and write the represented value.
    pub(super) fn set_update_value<T: PartialEq>(
        &self,
        handle: ObjectHandle,
        field: usize,
        current: &mut T,
        value: T,
    ) -> Result<bool> {
        self.write_update_fields(handle, field..=field, || {
            if *current == value {
                return false;
            }
            *current = value;
            true
        })
    }

    /// Object.cpp:970-995. Compare the resulting flag word.
    pub(super) fn update_flags(
        &self,
        handle: ObjectHandle,
        field: usize,
        current: &mut u32,
        flag: u32,
        set: bool,
    ) -> Result<()> {
        let value = if set {
            *current | flag
        } else {
            *current & !flag
        };
        self.set_update_value(handle, field, current, value)?;
        Ok(())
    }

    /// Unit.h:2576, Object.h:448, Object.cpp:859-869.
    pub(super) fn set_unit_target(
        &self,
        unit: ObjectHandle,
        current: &mut Option<ObjectHandle>,
        target: Option<ObjectHandle>,
    ) -> Result<()> {
        let guid = |unit| {
            self.get(unit)?
                .guid
                .filter(|guid| guid.raw() != 0)
                .ok_or_else(|| refuse("execution.unit.Unit_2576.target_guid_owner"))
        };
        let changed = match (*current, target) {
            (Some(old), Some(new)) if old != new => guid(old)? != guid(new)?,
            (None, Some(new)) => guid(new)?.raw() != 0,
            _ => *current != target,
        };
        self.write_update_fields(unit, UNIT_FIELD_TARGET..=UNIT_FIELD_TARGET + 1, || {
            if changed {
                *current = target;
            }
            changed
        })?;
        Ok(())
    }

    /// Object.cpp:1119-1127 also marks when the current value is unchanged.
    pub(super) fn force_update_field(&self, handle: ObjectHandle, field: usize) -> Result<()> {
        self.write_update_fields(handle, field..=field, || true)?;
        Ok(())
    }

    fn write_update_fields(
        &self,
        handle: ObjectHandle,
        fields: RangeInclusive<usize>,
        write_if_changed: impl FnOnce() -> bool,
    ) -> Result<bool> {
        let object = self.get(handle)?;
        require(
            *fields.end() < object.changed_fields.count()?,
            "execution.object.Object_849.update_field_index_assert",
        )?;
        if !write_if_changed() {
            return Ok(false);
        }
        Self::mark_for_client_update(object, &self.client_updates, fields)?;
        Ok(true)
    }

    fn mark_for_client_update(
        object: &SourceObject,
        client_updates: &RefCell<BTreeMap<MapIdentity, BTreeSet<ObjectHandle>>>,
        fields: RangeInclusive<usize>,
    ) -> Result<()> {
        let bits = object.changed_fields.words();
        for field in fields {
            let word = &bits[field / UPDATE_WORD_BITS];
            word.set(word.get() | (1 << (field % UPDATE_WORD_BITS)));
        }
        // Object.cpp:1107-1117,2486-2489; Map.h:287-290.
        if object.in_world && !object.object_updated.get() {
            let map = object
                .map
                .ok_or_else(|| refuse("execution.object.Object_2488.client_update_map_owner"))?;
            client_updates
                .borrow_mut()
                .entry(map)
                .or_default()
                .insert(object.handle);
            object.object_updated.set(true);
        }
        Ok(())
    }

    pub fn client_update_pending(&self, map: MapIdentity, object: ObjectHandle) -> bool {
        self.client_updates
            .borrow()
            .get(&map)
            .is_some_and(|pending| pending.contains(&object))
    }

    /// Map.cpp:2409-2413. Live aura, addon, combat, movement, and object writes mark in place.
    /// Fresh-only, respawn, and fishing writes cannot feed this drain on the represented paths.
    /// Refuse pending a drain owner; two or more members need the source pointer order.
    pub fn send_object_updates(&self, map: MapIdentity) -> Result<()> {
        if self
            .client_updates
            .borrow()
            .get(&map)
            .is_some_and(|set| set.len() > 1)
        {
            return Err(refuse(
                "execution.map.Map_2409.client_update_pointer_order_owner",
            ));
        }
        Err(refuse("execution.map.Map_2409.client_update_writers_owner"))
    }

    /// Object.cpp:719-733. `remove` is false on AddToWorld and true on RemoveFromWorld.
    fn clear_update_mask(&mut self, handle: ObjectHandle, remove: bool) -> Result<()> {
        let object = self
            .objects
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.stale_object_handle"))?;
        for word in object.changed_fields.words() {
            word.set(0);
        }
        if object.object_updated.get() {
            if remove {
                let map = object.map.ok_or_else(|| {
                    refuse("execution.object.Object_2493.client_update_map_owner")
                })?;
                if let Some(pending) = self.client_updates.get_mut().get_mut(&map) {
                    pending.remove(&handle);
                }
            }
            object.object_updated.set(false);
        }
        Ok(())
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
        let object = self
            .objects
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.stale_object_handle"))?;
        require(
            !object.in_world
                && !object.global_player_registered
                && object.kind != SourceObjectKind::Camera,
            "execution.object_guid_assignment",
        )?;
        let changed = object.guid.unwrap_or_default() != guid;
        object.guid = Some(guid);
        object.database_guid = database_guid;
        if changed && !matches!(object.changed_fields, SourceUpdateMask::Unbound) {
            // Object::_Create calls SetGuidValue; Object.cpp:115-125,859-869.
            Self::mark_for_client_update(
                object,
                &self.client_updates,
                OBJECT_FIELD_GUID..=OBJECT_FIELD_GUID + 1,
            )?;
        }
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
        cells.validate_cell(cell)?;
        cells.grid_remove(object)?;
        cells.grid_add(object, cell, |member| self.objects.contains_key(&member))?;
        cells.link(handle, kind, cell)?;
        if matches!(kind, SourceObjectKind::Corpse | SourceObjectKind::Bones) {
            self.get_mut(handle)?.corpse_grid_reference = Some(cell);
        }
        if kind.tracks_current_cell() {
            self.get_mut(handle)?.current_cell = Some(cell);
        }
        Ok(())
    }
    pub fn unlink_cell(&mut self, handle: ObjectHandle, cells: &mut SourceCells) -> Result<()> {
        let object = self.get(handle)?;
        require(object.map == Some(cells.map()), "execution.object_cell_map")?;
        require(
            object
                .corpse_grid_reference
                .is_none_or(|cell| cell.grid.map == cells.map()),
            "execution.corpse.cross_map_cell_owner",
        )?;
        cells.grid_remove(object)?;
        cells.unlink(handle);
        self.get_mut(handle)?.corpse_grid_reference = None;
        Ok(())
    }
    pub(super) fn reset_map(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get_mut(handle)?;
        require(!object.in_world, "execution.residency.reset_in_world")?;
        object.map = None;
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
    // ObjectAccessor.cpp:102-111. Global FindPlayer defaults to inWorld=true.
    pub(super) fn find_player(&self, guid: SourceGuid) -> Result<Option<ObjectHandle>> {
        if guid.raw() == 0 {
            return Ok(None);
        }
        let Some(handle) = self
            .lookup
            .get(&(None, LiveLookupKind::Player, guid))
            .copied()
        else {
            return Ok(None);
        };
        Ok(self.get(handle)?.in_world.then_some(handle))
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
        if object.in_world {
            return Ok(());
        }
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
        // WorldObject::AddToWorld adds notify membership before the base call; Object.cpp:1972-1981.
        if object.notify_on_event {
            let map = object
                .map
                .ok_or_else(|| refuse("execution.object.Object_1975.map_owner"))?;
            self.insert_event_notification(map, handle, object.source_allocation)?;
        }
        self.get_mut(handle)?.in_world = true;
        self.clear_update_mask(handle, false)
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
            return self.clear_update_mask(handle, true);
        }
        let kind = object.kind;
        let database_key = object
            .map
            .zip(kind.database_kind())
            .map(|(map, kind)| (map, kind, object.database_guid));
        let event_map = if object.notify_on_event {
            Some(
                object
                    .map
                    .ok_or_else(|| refuse("execution.object.Object_1989.map_owner"))?,
            )
        } else {
            None
        };
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
        if let Some(members) = event_map.and_then(|map| self.event_notified.get_mut(&map)) {
            members.remove(handle);
        }
        self.clear_update_mask(handle, true)?;
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
    // Map::GetWorldObject filters world membership only for players/corpses; Map.cpp:2382-2402.
    pub(super) fn lookup_world_object(
        &self,
        map: MapIdentity,
        guid: SourceGuid,
    ) -> Option<ObjectHandle> {
        let kind = match guid.high() {
            0x0000 => LiveLookupKind::Player,
            0xf110 | 0xf120 => LiveLookupKind::GameObject,
            0xf130 => LiveLookupKind::Creature,
            0xf140 => LiveLookupKind::Pet,
            0xf100 => LiveLookupKind::DynamicObject,
            0xf101 => LiveLookupKind::Corpse,
            _ => return None,
        };
        if matches!(kind, LiveLookupKind::Player | LiveLookupKind::Corpse) {
            return self.lookup(map, kind, guid);
        }
        self.lookup
            .get(&(Some(map), kind, guid))
            .copied()
            .filter(|handle| {
                self.objects
                    .get(handle)
                    .is_some_and(|object| object.map == Some(map))
            })
    }
    // Map.cpp:2371-2377,2331-2340. Reuse the same typed map stores and player filter.
    pub(super) fn lookup_unit(&self, map: MapIdentity, guid: SourceGuid) -> Option<ObjectHandle> {
        match guid.high() {
            0 | 0xf130 | 0xf140 if guid.raw() != 0 => self.lookup_world_object(map, guid),
            _ => None,
        }
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
        require(
            matches!(
                object.kind,
                SourceObjectKind::Creature | SourceObjectKind::Pet
            ),
            "execution.creature_relocation_kind",
        )?;
        let old = cells
            .registered_cell(handle)
            .ok_or_else(|| refuse("execution.creature_not_registered"))?;
        require(target.validate(), "execution.cell_coordinates")?;
        if (old.grid.x, old.grid.y) != (target.grid_x(), target.grid_y()) {
            if !object.active && !cells.object_data_loaded(target.grid_x(), target.grid_y()) {
                return Ok(CellRelocation::RefusedUnloaded);
            }
            load_at_enter(self, cells, target)?;
            require(
                cells.object_data_loaded(target.grid_x(), target.grid_y()),
                "execution.relocation_loader_not_complete",
            )?;
        }
        let new = cells.cell(target)?;
        if new == old {
            return Ok(CellRelocation::SameCell);
        }
        // Re-read live flags and allocation after the reentrant entry loader.
        self.link_cell(handle, new, cells)?;
        Ok(CellRelocation::Moved { old, new })
    }
    // A failed startup constructor has no map or grid reference; Corpse.cpp:153-252, ObjectMgr.cpp:7396-7400.
    pub(super) fn destroy_unbound(&mut self, handle: ObjectHandle) -> Result<()> {
        let object = self.get(handle)?;
        require(
            object.map.is_none()
                && !object.in_world
                && !object.global_player_registered
                && !object.player_map_linked
                && object.current_cell.is_none()
                && object.corpse_grid_reference.is_none(),
            "execution.destroy_bound_object",
        )?;
        if let Some(base) = object.allocation_base {
            self.allocations.remove(&base);
        }
        self.objects.remove(&handle);
        Ok(())
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
                && !cells.active().contains(handle),
            "execution.destroy_registered_object",
        )?;
        require(
            object.map.is_none_or(|map| map == cells.map()),
            "execution.object_cell_map",
        )?;
        require(
            object
                .corpse_grid_reference
                .is_none_or(|cell| cell.grid.map == cells.map()),
            "execution.corpse.cross_map_cell_owner",
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

impl SourceObjects {
    // Object.cpp:2583-2597 writes the flag before changing map membership.
    pub(super) fn set_notify_on_event_state(
        &mut self,
        handle: ObjectHandle,
        notify: bool,
    ) -> Result<()> {
        let object = self.get_mut(handle)?;
        if object.notify_on_event == notify {
            return Ok(());
        }
        object.notify_on_event = notify;
        if !object.in_world {
            return Ok(());
        }
        let map = object
            .map
            .ok_or_else(|| refuse("execution.object.Object_2594.map_owner"))?;
        if notify {
            let pointer = object.source_allocation;
            self.insert_event_notification(map, handle, pointer)?;
        } else if let Some(members) = self.event_notified.get_mut(&map) {
            members.remove(handle);
        }
        Ok(())
    }

    pub fn event_notified(&self, map: MapIdentity, handle: ObjectHandle) -> bool {
        self.event_notified
            .get(&map)
            .is_some_and(|members| members.contains(handle))
    }

    fn insert_event_notification(
        &mut self,
        map: MapIdentity,
        handle: ObjectHandle,
        pointer: Option<u64>,
    ) -> Result<()> {
        self.event_notified.entry(map).or_default().insert(
            handle,
            pointer,
            "execution.map.Map_1644.event_notify_pointer_duplicate",
            |_| Ok(()),
        )?;
        Ok(())
    }

    pub fn bind_event_notification_address(
        &mut self,
        map: MapIdentity,
        handle: ObjectHandle,
        pointer: u64,
    ) -> Result<()> {
        self.event_notified
            .get_mut(&map)
            .ok_or_else(|| refuse("execution.map.Map_2983.event_notify_member"))?
            .bind(handle, pointer)
    }

    pub(super) fn for_each_event_subscriber(
        &self,
        map: MapIdentity,
        order: EventNotificationOrder,
        mut visit: impl FnMut(ObjectHandle) -> Result<()>,
    ) -> Result<()> {
        let Some(members) = self.event_notified.get(&map) else {
            return Ok(());
        };
        // Deviation: At Map.cpp:2983 solo walks stable object identities because C++ addresses are not available.
        match order {
            EventNotificationOrder::Identity => {
                for (member, _) in members.bindings() {
                    visit(member)?;
                }
            }
            EventNotificationOrder::SourcePointer => {
                if members.len() > 1 {
                    members.ordered("execution.map.Map_2983.event_notify_pointer_order_owner")?;
                }
                for member in members.members() {
                    visit(member)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum EventNotificationOrder {
    Identity,
    SourcePointer,
}

#[cfg(test)]
mod player_lookup_tests {
    use super::*;

    #[test]
    fn pet_find_player_refuses_unavailable_registered_player_world_membership() {
        let mut objects = SourceObjects::default();
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        let guid = SourceGuid::without_entry(HighGuid::Player, 1);
        objects.assign_guid(player, 0, guid).unwrap();
        objects.register_player_lookup(player).unwrap();
        objects.objects.remove(&player);
        assert_eq!(
            objects.find_player(guid).unwrap_err().id,
            "execution.stale_object_handle"
        );
    }
}
