// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; SpawnGroup.cpp:739-1433,1474-1503.
use super::*;
use benilla_sim::source_spawn_groups::{Formation, GroupMember};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FormationSlotType {
    Static,
    Script,
    Player,
}
impl FormationSlotType {
    pub fn removable(self) -> bool {
        self != Self::Static
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FormationSlotIdentity {
    pub group: GroupIdentity,
    pub id: u32,
    pub allocation: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct FormationSlot {
    pub identity: FormationSlotIdentity,
    pub real_owner_guid: u32,
    pub kind: FormationSlotType,
    pub owner: Option<ObjectHandle>,
    pub can_follow: bool,
    pub recompute_position: bool,
}
#[derive(Clone, Debug)]
pub struct FormationDefinition {
    pub entry: Formation,
    pub dynamic: bool,
}
#[derive(Clone, Debug)]
pub struct FormationState {
    pub group: GroupIdentity,
    pub definition: u64,
    pub members: Vec<GroupMember>,
    pub slots: BTreeMap<u32, Option<FormationSlotIdentity>>,
    pub slot_guid: u32,
    pub real_master_guid: Option<u32>,
    pub shape: u32,
    pub spread_bits: u32,
    pub options: u32,
    pub motion: u32,
    pub spawn_position: Option<[f32; 4]>,
    pub mirror: bool,
    pub followers_stopped: bool,
    pub master_died: bool,
    pub follower_start_millis: Option<i64>,
    pub last_waypoint: u32,
    pub waypoint_path: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct FormationUnit {
    pub database_guid: u32,
    pub player: bool,
    pub alive: bool,
    pub temporary_summon: bool,
    pub moving: bool,
    pub respawn_position: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormationMotion {
    Clear {
        reset: bool,
        all: bool,
    },
    Initialize,
    Waypoint {
        path: u32,
        linear: bool,
    },
    NextWaypoint(u32),
    Idle,
    Follow {
        slot: FormationSlotIdentity,
        master: ObjectHandle,
    },
}
impl FormationMotion {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::Follow { .. } => "execution.formation.f5_motion_follow_owner",
            Self::Initialize => "execution.formation.f5_motion_initialize_owner",
            Self::Clear { reset: false, .. } => "execution.formation.f5_motion_clear_owner",
            _ => "execution.formation.f5_motion_master_owner",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FormationCommand {
    Attach(ObjectHandle, FormationSlotType),
    AddCreature(ObjectHandle),
    AddPlayer(ObjectHandle),
    Remove(ObjectHandle),
    Death(ObjectHandle),
    Delete(ObjectHandle),
    Home,
    Reset,
    Disband,
    TryMaster(Option<ObjectHandle>),
    BindFollowers,
    ClearMotion,
    StartFollowers,
    StopFollowers,
    Update,
    ResetLastWaypoint,
    WaypointStart,
    WaypointEnd,
    Shape(u32),
    Spread(u32),
    Options(u32),
    Compact(bool),
    Movement { kind: u32, path: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormationOutcome {
    Done,
    Changed(bool),
    Slot(Option<FormationSlotIdentity>),
}
/// Live Unit, clock and motion owners execute at their source call, never from recorded results.
pub trait FormationOwner {
    fn formation_unit(&mut self, object: ObjectHandle) -> Result<FormationUnit>;
    fn formation_creature(
        &mut self,
        map: MapIdentity,
        database_guid: u32,
    ) -> Result<Option<ObjectHandle>>;
    fn formation_out_of_combat(&mut self, group: GroupIdentity) -> Result<bool>;
    fn formation_world_millis(&mut self) -> Result<i64>;
    fn formation_waypoint(&mut self, object: ObjectHandle) -> Result<(u32, u32)>;
    fn formation_follow_target(&mut self, object: ObjectHandle) -> Result<Option<ObjectHandle>>;
    fn formation_motion(&mut self, object: ObjectHandle, motion: FormationMotion) -> Result<()> {
        let _ = object;
        Err(motion.missing_owner())
    }
    fn formation_random_motion(
        &mut self,
        _object: ObjectHandle,
        _position: [f32; 4],
    ) -> Result<()> {
        Err(refuse("execution.formation.f5_motion_master_owner"))
    }
    fn formation_positions(
        &mut self,
        _formations: &mut SourceFormations,
        _group: GroupIdentity,
    ) -> Result<()> {
        Err(refuse("execution.formation.f4_13b_shape_numerics_owner"))
    }
    fn formation_variation(
        &mut self,
        _formations: &mut SourceFormations,
        _group: GroupIdentity,
    ) -> Result<()> {
        Err(refuse(
            "execution.formation.f4_13b_periodic_variation_owner",
        ))
    }
}
#[derive(Default)]
pub struct SourceFormations {
    states: BTreeMap<GroupIdentity, FormationState>,
    definitions: BTreeMap<u64, FormationDefinition>,
    static_definitions: BTreeMap<u32, u64>,
    slots: BTreeMap<FormationSlotIdentity, FormationSlot>,
    bindings: BTreeMap<ObjectHandle, FormationSlotIdentity>,
    next_allocation: u64,
    stopped: bool,
}
impl SourceFormations {
    pub fn state(&self, group: GroupIdentity) -> Result<&FormationState> {
        self.states
            .get(&group)
            .ok_or_else(|| refuse("execution.formation.absent"))
    }
    fn state_mut(&mut self, group: GroupIdentity) -> Result<&mut FormationState> {
        self.states
            .get_mut(&group)
            .ok_or_else(|| refuse("execution.formation.absent"))
    }
    pub fn slot(&self, id: FormationSlotIdentity) -> Result<&FormationSlot> {
        self.slots
            .get(&id)
            .ok_or_else(|| refuse("execution.formation.stale_slot"))
    }
    pub fn slot_mut(&mut self, id: FormationSlotIdentity) -> Result<&mut FormationSlot> {
        self.slots
            .get_mut(&id)
            .ok_or_else(|| refuse("execution.formation.stale_slot"))
    }
    pub fn binding(&self, object: ObjectHandle) -> Option<FormationSlotIdentity> {
        self.bindings.get(&object).copied()
    }
    pub fn definition(&self, group: GroupIdentity) -> Result<&FormationDefinition> {
        Ok(&self.definitions[&self.state(group)?.definition])
    }
    fn allocation(&mut self) -> Result<u64> {
        self.next_allocation = self
            .next_allocation
            .checked_add(1)
            .ok_or_else(|| refuse("execution.formation.allocation_overflow"))?;
        Ok(self.next_allocation)
    }
    pub fn create(
        &mut self,
        group: GroupIdentity,
        entry: Formation,
        members: &[GroupMember],
        dynamic: bool,
    ) -> Result<()> {
        require(!self.stopped, "execution.formation.stopped")?;
        require(
            !self.states.contains_key(&group),
            "execution.formation.creation_repeated",
        )?;
        require(
            entry.group == group.handle.id,
            "execution.formation.definition_binding",
        )?;
        let definition = if !dynamic {
            self.static_definitions.get(&entry.group).copied()
        } else {
            None
        };
        let definition = match definition {
            Some(id) => id,
            None => {
                let id = self.allocation()?;
                if !dynamic {
                    self.static_definitions.insert(entry.group, id);
                }
                self.definitions.insert(
                    id,
                    FormationDefinition {
                        entry: entry.clone(),
                        dynamic,
                    },
                );
                id
            }
        };
        let entry = &self.definitions[&definition].entry;
        self.states.insert(
            group,
            FormationState {
                group,
                definition,
                members: members.to_vec(),
                slots: BTreeMap::new(),
                slot_guid: 0,
                real_master_guid: None,
                shape: entry.kind,
                spread_bits: entry.spread_bits,
                options: entry.options,
                motion: entry.movement,
                spawn_position: None,
                mirror: false,
                followers_stopped: false,
                master_died: false,
                follower_start_millis: None,
                last_waypoint: 0,
                waypoint_path: 0,
            },
        );
        // Duplicate keys keep the first slot but the last slot-zero GUID; SpawnGroup.cpp:742-754.
        for member in members {
            let id = member.slot as u32;
            self.insert_slot(group, id, member.guid, FormationSlotType::Static)?;
            if id == 0 {
                self.state_mut(group)?.real_master_guid = Some(member.guid);
            }
        }
        self.state_mut(group)?.slot_guid = self.state(group)?.slots.len() as u32;
        Ok(())
    }
    // Replacement constructs before Reset; clearing Disbands before releasing; SpawnGroup.cpp:596-606.
    pub fn set_definition(
        &mut self,
        group: GroupIdentity,
        entry: Option<Formation>,
        members: &[GroupMember],
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        require(!self.stopped, "execution.formation.stopped")?;
        if let Some(entry) = entry {
            self.states.remove(&group);
            let result = self
                .create(group, entry, members, true)
                .and_then(|()| self.reset(group, owner));
            if result.is_err() {
                self.stopped = true;
            }
            result
        } else {
            self.execute(group, FormationCommand::Disband, owner)?;
            self.states.remove(&group);
            Ok(())
        }
    }
    pub fn can_use_mmap(&self, group: GroupIdentity) -> Result<bool> {
        Ok(self.state(group)?.options & 1 == 0)
    }
    fn insert_slot(
        &mut self,
        group: GroupIdentity,
        id: u32,
        guid: u32,
        kind: FormationSlotType,
    ) -> Result<bool> {
        let allocation = self.allocation()?;
        if self.state(group)?.slots.contains_key(&id) {
            return Ok(false);
        }
        let identity = FormationSlotIdentity {
            group,
            id,
            allocation,
        };
        self.slots.insert(
            identity,
            FormationSlot {
                identity,
                real_owner_guid: guid,
                kind,
                owner: None,
                can_follow: true,
                recompute_position: true,
            },
        );
        self.state_mut(group)?.slots.insert(id, Some(identity));
        Ok(true)
    }
    // FormationSlotMap is ordered by uint32 slot ID; SpawnGroupDefines.h:36.
    fn next_slot(
        &self,
        group: GroupIdentity,
        after: &mut Option<u32>,
    ) -> Result<Option<FormationSlotIdentity>> {
        use std::ops::Bound::{Excluded, Unbounded};
        let lower = after.map_or(Unbounded, Excluded);
        match self.state(group)?.slots.range((lower, Unbounded)).next() {
            None => Ok(None),
            Some((&key, value)) => {
                *after = Some(key);
                value
                    .map(Some)
                    .ok_or_else(|| refuse("execution.formation.null_slot_undefined"))
            }
        }
    }
    pub fn master(&self, group: GroupIdentity) -> Result<Option<ObjectHandle>> {
        match self.state(group)?.slots.get(&0) {
            None => Ok(None),
            Some(Some(id)) => Ok(self.slot(*id)?.owner),
            Some(None) => Err(refuse("execution.formation.null_slot_undefined")),
        }
    }
    pub fn is_master(&self, slot: FormationSlotIdentity) -> Result<bool> {
        // Two null pointers compare equal, including an empty nonzero slot; SpawnGroup.cpp:1861-1864.
        Ok(self.master(slot.group)? == self.slot(slot)?.owner)
    }
    fn default_slot(
        &mut self,
        group: GroupIdentity,
        guid: u32,
        kind: FormationSlotType,
    ) -> Result<Option<FormationSlotIdentity>> {
        if kind == FormationSlotType::Static {
            let id = self
                .state(group)?
                .members
                .iter()
                .find(|m| m.guid == guid)
                .map(|m| m.slot);
            return Ok(match id {
                Some(id) if id >= 0 => *self
                    .state_mut(group)?
                    .slots
                    .entry(id as u32)
                    .or_insert(None),
                _ => None,
            });
        }
        let mut cursor = None;
        while let Some(id) = self.next_slot(group, &mut cursor)? {
            let slot = self.slot(id)?;
            if slot.kind == kind && slot.real_owner_guid == guid {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }
    fn set_owner(
        &mut self,
        slot: FormationSlotIdentity,
        owner: Option<ObjectHandle>,
    ) -> Result<()> {
        self.slot_mut(slot)?.owner = owner;
        if let Some(owner) = owner {
            self.bindings.insert(owner, slot);
        }
        Ok(())
    }
    fn swap(&mut self, a: FormationSlotIdentity, b: FormationSlotIdentity) -> Result<()> {
        let ao = self.slot(a)?.owner;
        let bo = self.slot(b)?.owner;
        self.set_owner(a, bo)?;
        self.set_owner(b, ao)
    }
    fn free_slot(
        &mut self,
        group: GroupIdentity,
        slot: FormationSlotIdentity,
        owner: &mut impl FormationOwner,
    ) -> Result<bool> {
        let Some(unit) = self.slot(slot)?.owner else {
            return Ok(true);
        };
        let guid = owner.formation_unit(unit)?.database_guid;
        let mut target = self.default_slot(group, guid, FormationSlotType::Static)?;
        if target.is_none() || target == Some(slot) {
            target = None;
            let mut cursor = None;
            while let Some(id) = self.next_slot(group, &mut cursor)? {
                let s = self.slot(id)?;
                if !s.kind.removable() && s.owner.is_none() {
                    target = Some(id);
                    break;
                }
            }
        }
        let Some(target) = target else {
            return Ok(false);
        };
        self.swap(slot, target)?;
        Ok(true)
    }
    fn positions(&mut self, group: GroupIdentity, owner: &mut impl FormationOwner) -> Result<()> {
        owner.formation_positions(self, group)
    }
    fn master_motion(
        &mut self,
        group: GroupIdentity,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        let Some(master) = self.master(group)? else {
            return Ok(());
        };
        owner.formation_motion(
            master,
            FormationMotion::Clear {
                reset: true,
                all: true,
            },
        )?;
        let state = self.state(group)?;
        let kind = state.motion;
        let path = self.definition(group)?.entry.path;
        if kind == 2 || kind == 4 {
            owner.formation_motion(
                master,
                FormationMotion::Waypoint {
                    path,
                    linear: kind == 4,
                },
            )?;
            owner.formation_motion(
                master,
                FormationMotion::NextWaypoint(self.state(group)?.last_waypoint.wrapping_add(1)),
            )?;
            self.state_mut(group)?.waypoint_path = 0;
            self.state_mut(group)?.last_waypoint = 0;
        } else if kind == 1 {
            let position = state.spawn_position.ok_or_else(|| {
                refuse("execution.formation.spawn_position_uninitialized_undefined")
            })?;
            owner.formation_random_motion(master, position)?;
        } else {
            owner.formation_motion(master, FormationMotion::Idle)?;
        }
        Ok(())
    }
    fn bind_followers(
        &mut self,
        group: GroupIdentity,
        owner: &mut impl FormationOwner,
    ) -> Result<bool> {
        let Some(master) = self.master(group)? else {
            return Ok(false);
        };
        let mut cursor = None;
        while let Some(slot) = self.next_slot(group, &mut cursor)? {
            if self.is_master(slot)? {
                continue;
            }
            if let Some(follower) = self.slot(slot)?.owner {
                if owner.formation_unit(follower)?.alive
                    && owner.formation_follow_target(follower)? != Some(master)
                {
                    owner.formation_motion(
                        follower,
                        FormationMotion::Clear {
                            reset: false,
                            all: true,
                        },
                    )?;
                    owner.formation_motion(follower, FormationMotion::Follow { slot, master })?;
                    self.slot_mut(slot)?.recompute_position = true;
                }
            }
        }
        // The source returns false even after binding every follower; SpawnGroup.cpp:807.
        Ok(false)
    }
    fn attach(
        &mut self,
        group: GroupIdentity,
        object: ObjectHandle,
        kind: FormationSlotType,
        owner: &mut impl FormationOwner,
    ) -> Result<Option<FormationSlotIdentity>> {
        let unit = owner.formation_unit(object)?;
        if !unit.alive {
            return Ok(None);
        }
        if let Some(slot) = self.binding(object) {
            return Ok(Some(slot));
        }
        let Some(slot) = self.default_slot(group, unit.database_guid, kind)? else {
            return Ok(None);
        };
        if !self.free_slot(group, slot, owner)? {
            return Ok(None);
        }
        self.set_owner(slot, Some(object))?;
        if slot.id == 0 {
            self.state_mut(group)?.real_master_guid = Some(unit.database_guid);
            self.state_mut(group)?.spawn_position = Some(unit.respawn_position);
            // Initial attachment moves even a dynamic master; SpawnGroup.cpp:1415-1424.
            self.master_motion(group, owner)?;
            let shape = self.definition(group)?.entry.kind;
            self.shape(group, shape, owner)?;
        }
        if self.master(group)?.is_some() {
            self.positions(group, owner)?;
            self.bind_followers(group, owner)?;
        }
        Ok(Some(slot))
    }
    fn allocate_guid(&mut self, group: GroupIdentity) -> Result<u32> {
        let id = self.state(group)?.slot_guid;
        self.state_mut(group)?.slot_guid = id.wrapping_add(1);
        Ok(id)
    }
    fn add_creature(
        &mut self,
        group: GroupIdentity,
        object: ObjectHandle,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        if self.binding(object).is_some() {
            return Ok(());
        }
        let unit = owner.formation_unit(object)?;
        if unit.temporary_summon || !unit.alive {
            return Ok(());
        }
        if self
            .default_slot(group, unit.database_guid, FormationSlotType::Script)?
            .is_none()
        {
            let id = self.allocate_guid(group)?;
            if !self.insert_slot(group, id, unit.database_guid, FormationSlotType::Script)? {
                return Ok(());
            }
        }
        self.attach(group, object, FormationSlotType::Script, owner)?;
        Ok(())
    }
    fn add_player(
        &mut self,
        group: GroupIdentity,
        object: ObjectHandle,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        if self.binding(object).is_some() {
            return Ok(());
        }
        // Players allocate before lookup and are not tested for liveness; SpawnGroup.cpp:1254-1277.
        let id = self.allocate_guid(group)?;
        if id == 0 {
            self.state_mut(group)?.slot_guid = 0;
            return Ok(());
        }
        let guid = owner.formation_unit(object)?.database_guid;
        let slot = match self.default_slot(group, guid, FormationSlotType::Player)? {
            Some(slot) => slot,
            None => {
                if !self.insert_slot(group, id, guid, FormationSlotType::Player)? {
                    return Ok(());
                }
                self.state(group)?.slots[&id].unwrap()
            }
        };
        self.set_owner(slot, Some(object))?;
        self.positions(group, owner)?;
        self.bind_followers(group, owner)?;
        Ok(())
    }
    fn followers(
        &mut self,
        group: GroupIdentity,
        start: bool,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        let mut cursor = None;
        while let Some(id) = self.next_slot(group, &mut cursor)? {
            self.slot_mut(id)?.can_follow = start;
        }
        self.state_mut(group)?.followers_stopped = !start;
        if !start {
            let now = owner.formation_world_millis()?;
            self.state_mut(group)?.follower_start_millis =
                Some(now.checked_add(5000).ok_or_else(|| {
                    refuse("execution.formation.follower_deadline_overflow_undefined")
                })?);
        }
        Ok(())
    }
    fn first_alive(
        &self,
        group: GroupIdentity,
        owner: &mut impl FormationOwner,
    ) -> Result<Option<FormationSlotIdentity>> {
        let mut cursor = None;
        while let Some(id) = self.next_slot(group, &mut cursor)? {
            let slot = self.slot(id)?;
            if !slot.kind.removable() {
                if let Some(unit) = slot.owner {
                    if owner.formation_unit(unit)?.alive {
                        return Ok(Some(id));
                    }
                }
            }
        }
        Ok(None)
    }
    fn try_master(
        &mut self,
        group: GroupIdentity,
        candidate: Option<ObjectHandle>,
        owner: &mut impl FormationOwner,
    ) -> Result<bool> {
        let Some(master_slot) = self.state(group)?.slots.get(&0).copied() else {
            return Ok(false);
        };
        let master_slot =
            master_slot.ok_or_else(|| refuse("execution.formation.null_slot_undefined"))?;
        let candidate_alive = match candidate {
            Some(unit) => owner.formation_unit(unit)?.alive,
            None => false,
        };
        let alive = if candidate_alive {
            self.binding(candidate.unwrap())
                .filter(|s| s.group.handle.id == group.handle.id)
        } else {
            self.first_alive(group, owner)?
        };
        let Some(alive) = alive else {
            return Ok(false);
        };
        self.followers(group, false, owner)?;
        self.swap(master_slot, alive)?;
        self.positions(group, owner)?;
        if !self.definition(group)?.dynamic {
            self.master_motion(group, owner)?;
        }
        self.bind_followers(group, owner)?;
        Ok(true)
    }
    fn death(
        &mut self,
        group: GroupIdentity,
        object: ObjectHandle,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        let Some(slot) = self.binding(object) else {
            return Ok(());
        };
        let master = self.is_master(slot)?;
        if master {
            let (wp, path) = owner.formation_waypoint(object)?;
            self.state_mut(group)?.last_waypoint = wp;
            self.state_mut(group)?.waypoint_path = path;
        }
        self.slot_mut(slot)?.owner = None;
        self.bindings.remove(&object);
        if master {
            if !owner.formation_out_of_combat(group)? {
                self.state_mut(group)?.master_died = true;
            } else {
                self.try_master(group, None, owner)?;
            }
        } else if self.state(group)?.options & 2 != 0 {
            self.positions(group, owner)?;
        }
        Ok(())
    }
    fn remove(
        &mut self,
        group: GroupIdentity,
        object: ObjectHandle,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        let Some(mut slot) = self.binding(object) else {
            return Ok(());
        };
        let unit = owner.formation_unit(object)?;
        if self.slot(slot)?.real_owner_guid != unit.database_guid {
            let mut found = None;
            let mut cursor = None;
            while let Some(id) = self.next_slot(group, &mut cursor)? {
                let s = self.slot(id)?;
                // Player removal skips PLAYER slots, not the other kinds; SpawnGroup.cpp:1345-1349.
                if unit.player && s.kind == FormationSlotType::Player {
                    continue;
                }
                if s.real_owner_guid == unit.database_guid {
                    found = Some(id);
                    break;
                }
            }
            if let Some(target) = found {
                self.swap(slot, target)?;
                slot = target;
            } else if unit.player {
                return Ok(());
            } else {
                self.death(group, object, owner)?;
            }
        }
        if unit.player {
            if self.slot(slot)?.kind.removable() {
                self.state_mut(group)?.slots.remove(&slot.id);
            } else {
                self.slot_mut(slot)?.owner = None;
            }
            self.bindings.remove(&object);
            owner.formation_motion(object, FormationMotion::Initialize)?;
        } else {
            self.death(group, object, owner)?;
            owner.formation_motion(object, FormationMotion::Initialize)?;
            if self.slot(slot)?.kind.removable() {
                self.state_mut(group)?.slots.remove(&slot.id);
            }
        }
        if self.state(group)?.options & 2 != 0 {
            self.positions(group, owner)?;
        }
        Ok(())
    }
    fn shape(
        &mut self,
        group: GroupIdentity,
        shape: u32,
        owner: &mut impl FormationOwner,
    ) -> Result<bool> {
        if self.state(group)?.shape == shape {
            return Ok(false);
        }
        self.state_mut(group)?.shape = shape;
        self.positions(group, owner)?;
        Ok(true)
    }
    fn clear_motion(
        &mut self,
        group: GroupIdentity,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        let mut cursor = None;
        while let Some(slot) = self.next_slot(group, &mut cursor)? {
            if let Some(unit) = self.slot(slot)?.owner {
                if !owner.formation_unit(unit)?.alive {
                    continue;
                }
                if self.is_master(slot)? {
                    if self.definition(group)?.dynamic {
                        continue;
                    }
                    let (wp, path) = owner.formation_waypoint(unit)?;
                    self.state_mut(group)?.last_waypoint = wp;
                    self.state_mut(group)?.waypoint_path = path;
                }
                owner.formation_motion(unit, FormationMotion::Initialize)?;
            }
        }
        Ok(())
    }
    fn reset(&mut self, group: GroupIdentity, owner: &mut impl FormationOwner) -> Result<()> {
        self.state_mut(group)?.mirror = false;
        self.state_mut(group)?.master_died = false;
        self.shape(group, self.definition(group)?.entry.kind, owner)?;
        let mut cursor = None;
        while let Some(slot) = self.next_slot(group, &mut cursor)? {
            if self.slot(slot)?.kind.removable() {
                if let Some(unit) = self.slot(slot)?.owner {
                    owner.formation_motion(unit, FormationMotion::Initialize)?;
                    self.bindings.remove(&unit);
                }
                self.state_mut(group)?.slots.remove(&slot.id);
            } else {
                let guid = self.slot(slot)?.real_owner_guid;
                let matches = if let Some(unit) = self.slot(slot)?.owner {
                    owner.formation_unit(unit)?.database_guid == guid
                } else {
                    false
                };
                if !matches {
                    if let Some(unit) = owner.formation_creature(group.map, guid)? {
                        if let Some(previous) = self.binding(unit) {
                            if previous != slot {
                                self.swap(slot, previous)?;
                            }
                        }
                        self.set_owner(slot, Some(unit))?;
                    } else {
                        self.slot_mut(slot)?.owner = None;
                    }
                }
            }
        }
        if self.state(group)?.slots.contains_key(&0) {
            self.try_master(group, self.master(group)?, owner)?;
        }
        self.followers(group, true, owner)
    }
    pub fn execute(
        &mut self,
        group: GroupIdentity,
        command: FormationCommand,
        owner: &mut impl FormationOwner,
    ) -> Result<FormationOutcome> {
        require(!self.stopped, "execution.formation.stopped")?;
        let result = self.execute_inner(group, command, owner);
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    fn execute_inner(
        &mut self,
        group: GroupIdentity,
        command: FormationCommand,
        owner: &mut impl FormationOwner,
    ) -> Result<FormationOutcome> {
        self.state(group)?;
        use FormationCommand as C;
        match command {
            C::Attach(object, kind) => {
                return self
                    .attach(group, object, kind, owner)
                    .map(FormationOutcome::Slot)
            }
            C::AddCreature(object) => self.add_creature(group, object, owner)?,
            C::AddPlayer(object) => self.add_player(group, object, owner)?,
            C::Remove(object) => self.remove(group, object, owner)?,
            C::Death(object) | C::Delete(object) => self.death(group, object, owner)?,
            C::Home => {
                if self.state(group)?.master_died {
                    self.state_mut(group)?.master_died = false;
                    self.try_master(group, None, owner)?;
                }
            }
            C::Reset => self.reset(group, owner)?,
            C::Disband => {
                self.clear_motion(group, owner)?;
                let mut cursor = None;
                while let Some(slot) = self.next_slot(group, &mut cursor)? {
                    if let Some(unit) = self.slot(slot)?.owner {
                        self.bindings.remove(&unit);
                    }
                }
                self.state_mut(group)?.slots.clear();
            }
            C::TryMaster(candidate) => {
                return self
                    .try_master(group, candidate, owner)
                    .map(FormationOutcome::Changed)
            }
            C::BindFollowers => {
                return self
                    .bind_followers(group, owner)
                    .map(FormationOutcome::Changed)
            }
            C::ClearMotion => self.clear_motion(group, owner)?,
            C::StartFollowers => self.followers(group, true, owner)?,
            C::StopFollowers => self.followers(group, false, owner)?,
            C::Update => {
                if self.state(group)?.followers_stopped {
                    let now = owner.formation_world_millis()?;
                    if self
                        .state(group)?
                        .follower_start_millis
                        .is_some_and(|deadline| deadline < now)
                    {
                        self.followers(group, true, owner)?;
                    }
                }
                if self.state(group)?.shape == 0 {
                    owner.formation_variation(self, group)?;
                }
            }
            C::ResetLastWaypoint => self.state_mut(group)?.last_waypoint = 0,
            C::WaypointStart => self.state_mut(group)?.mirror = false,
            C::WaypointEnd => self.state_mut(group)?.mirror = true,
            C::Shape(shape) => {
                return self
                    .shape(group, shape, owner)
                    .map(FormationOutcome::Changed)
            }
            C::Spread(bits) => {
                self.state_mut(group)?.spread_bits = bits;
                self.positions(group, owner)?;
            }
            C::Options(options) => {
                // Recompute sees the old options and assignment follows it; SpawnGroup.cpp:821-829.
                if (self.state(group)?.options ^ options) & 2 != 0 {
                    self.positions(group, owner)?;
                }
                self.state_mut(group)?.options = options;
            }
            C::Compact(set) => {
                let options = self.state(group)?.options;
                self.state_mut(group)?.options = if set { options | 2 } else { options & !2 };
                self.positions(group, owner)?;
            }
            C::Movement { kind, path } => {
                let definition = self.state(group)?.definition;
                let entry = &mut self.definitions.get_mut(&definition).unwrap().entry;
                entry.path = path;
                entry.movement = kind;
                self.state_mut(group)?.motion = kind;
                self.state_mut(group)?.last_waypoint = 0;
                if let Some(master) = self.master(group)? {
                    let unit = owner.formation_unit(master)?;
                    require(
                        !unit.player,
                        "execution.formation.player_respawn_downcast_undefined",
                    )?;
                    let mut position = unit.respawn_position;
                    if path != 0 {
                        position[3] = path as f32;
                    }
                    self.state_mut(group)?.spawn_position = Some(position);
                }
            }
        }
        Ok(FormationOutcome::Done)
    }
}
impl GroupCallback {
    pub fn formation(
        self,
        group: GroupIdentity,
        groups: &SourceSpawnGroups,
        formations: &mut SourceFormations,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        let command = match self {
            Self::FormationCreate => {
                groups.state(group)?;
                let definition = groups.formation_definition(group)?;
                return formations.create(group, definition.0.clone(), definition.1, false);
            }
            Self::FormationUpdate => FormationCommand::Update,
            Self::FormationHome => FormationCommand::Home,
            Self::FormationRemove { object } => FormationCommand::Remove(object),
            Self::ResetLastWaypoint => FormationCommand::ResetLastWaypoint,
            _ => return Err(self.missing_owner()),
        };
        formations.execute(group, command, owner).map(|_| ())
    }
}
impl CreaturePhase {
    pub fn formation(
        self,
        group: GroupIdentity,
        creature: &CreatureConstruction,
        formations: &mut SourceFormations,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        require(
            self == Self::FormationSlot && creature.group.is_some_and(|g| g.id == group.handle.id),
            "execution.formation.constructor_binding",
        )?;
        formations
            .execute(
                group,
                FormationCommand::Attach(creature.object, FormationSlotType::Static),
                owner,
            )
            .map(|_| ())
    }
}
