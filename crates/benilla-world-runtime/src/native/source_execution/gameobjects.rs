// SPDX-License-Identifier: GPL-3.0-or-later
//! Source GO construction with mandatory executable manager, geometry and script owners.
use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GameObjectTemplate {
    pub entry: u32,
    pub kind: u32,
    pub display: u32,
    pub faction: u32,
    pub flags: u32,
    pub extra_flags: u32,
    pub scale_bits: u32,
    pub data: [u32; 24],
    pub string_id: u32,
}
impl GameObjectTemplate {
    // Linked entries use distinct union offsets; GameObject.h:468-477.
    pub fn linked_entry(&self) -> u32 {
        match self.kind {
            1 => self.data[3],
            3 => self.data[7],
            8 => self.data[2],
            10 => self.data[12],
            _ => 0,
        }
    }
    fn permanent(&self) -> bool {
        let no_damage_immune = match self.kind {
            0 => self.data[3],
            1 => self.data[4],
            10 => self.data[11],
            24 => self.data[5],
            26 => self.data[3],
            _ => return false,
        };
        no_damage_immune == 0 && !(self.kind == 10 && self.data[5] != 0)
    }
    fn server_only(&self) -> bool {
        match self.kind {
            5 => self.data[2] != 0,
            6 => self.data[8] != 0,
            8 => self.data[3] != 0,
            30 => self.data[6] != 0,
            _ => false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct GameObjectSpawn {
    pub database_guid: u32,
    pub entry: u32,
    pub position_bits: [u32; 4],
    pub rotation_bits: [u32; 4],
    pub animation: u32,
    // Retain signed source storage until SetGoState writes the byte; GameObject.h:590-594, GameObject.cpp:926-927,2077-2079.
    pub state: i32,
    pub respawn_min: i32,
    pub respawn_max: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct GameObjectGroupEntry {
    pub id: u32,
    pub string_id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameObjectFactoryKind {
    Ordinary,
    Elevator,
}
// Factory selection uses the database entry, before random/group resolution; ObjectGridLoader.cpp:123-129, GameObject.cpp:107-112.
pub fn source_gameobject_factory(template: Option<&GameObjectTemplate>) -> GameObjectFactoryKind {
    if template.is_some_and(|template| template.kind == 11) {
        GameObjectFactoryKind::Elevator
    } else {
        GameObjectFactoryKind::Ordinary
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameObjectCallback {
    TemplateString(u32),
    CreateBattlegroundOrOutdoor,
    CreateInstance,
    LargeVisibility,
    BindGroup(u32),
    GroupString(u32),
    MapViewpoint,
    MapVisibility,
    InitializeAi,
    RemoveOutdoorAndOwner,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameObjectModelOperation {
    ConstructAndInsert,
    Contains,
    Relocate,
    Insert,
    Enable(bool),
    Remove,
    Destroy,
}
#[derive(Debug)]
pub struct SourceGameObject {
    pub handle: ObjectHandle,
    pub template: GameObjectTemplate,
    pub display: u32,
    pub position_bits: [u32; 4],
    pub rotation_bits: [u32; 4],
    pub animation: u8,
    pub state: u8,
    pub loot_state: u8,
    pub flags: u32,
    pub dynamic_flags: u32,
    pub level: u32,
    pub stealth_strength: u32,
    pub invisibility_value: u32,
    pub spawned_by_default: bool,
    pub respawn_delay: u32,
    pub respawn_time: i64,
    pub linked_trap: Option<SourceGuid>,
    pub spell: u32,
    pub owner: SourceGuid,
    pub has_model: bool,
    pub spawn: Option<GameObjectSpawn>,
    pub group: Option<GameObjectGroupEntry>,
    pub respawn_overridden: bool,
    pub respawn_override_once: bool,
    pub forced_despawn: bool,
    pub has_loot: bool,
    pub corpse_despawn_ms: i64,
    pub restock_time: i64,
    pub cooldown_time: i64,
    pub rearm_time: i64,
    pub capture_timer: u32,
    pub delayed_action_timer: u32,
    pub use_times: u32,
    pub first_user: SourceGuid,
    pub unique_users: std::collections::BTreeSet<SourceGuid>,
    pub skillup_users: std::collections::BTreeSet<SourceGuid>,
    pub(super) lifecycle_stopped: bool,
}
impl SourceGameObject {
    // Spawn and collision predicates remain separate from world membership; GameObject.h:763-767, GameObject.cpp:1317-1334.
    pub fn is_spawned(&self) -> bool {
        self.respawn_delay == 0
            || (self.respawn_time > 0 && !self.spawned_by_default)
            || (self.respawn_time == 0 && self.spawned_by_default)
    }
    pub fn collision_enabled(&self) -> bool {
        self.is_spawned()
            && !self.template.server_only()
            && match self.template.kind {
                0 => self.state == 1,
                6 => false,
                _ => true,
            }
    }
    pub fn set_spell_id(&mut self, spell: u32) {
        // Summoned spell state is nondefault even for spell zero; GameObject.h:739-743.
        self.spawned_by_default = false;
        self.spell = spell;
    }
    pub fn set_owner_guid(&mut self, owner: SourceGuid) {
        // Owner assignment always switches to nondefault lifetime; GameObject.cpp:1002-1006.
        self.spawned_by_default = false;
        self.owner = owner;
    }
}
#[derive(Default)]
pub struct SourceGameObjects {
    objects: BTreeMap<ObjectHandle, SourceGameObject>,
}
impl SourceGameObjects {
    pub fn get(&self, handle: ObjectHandle) -> Result<&SourceGameObject> {
        self.objects
            .get(&handle)
            .ok_or_else(|| refuse("execution.stale_gameobject"))
    }
    pub fn get_mut(&mut self, handle: ObjectHandle) -> Result<&mut SourceGameObject> {
        self.objects
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.stale_gameobject"))
    }
    // Release GO fields only after the authoritative construction is destroyed; GameObject.cpp:102-105, Object.cpp:61-68.
    pub fn forget_destroyed(
        &mut self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<()> {
        require(
            objects.get(handle).is_err(),
            "execution.gameobject_still_constructed",
        )?;
        self.objects
            .remove(&handle)
            .ok_or_else(|| refuse("execution.stale_gameobject"))?;
        Ok(())
    }
}
// All dependency methods execute at their actual source call; no captured outcome is an implementation.
pub trait GameObjectFactoryOwner<D: GridDefinitions, P: GridProductOwner>:
    GridObjectFactory<D, P>
{
    fn gameobjects(&mut self) -> &mut SourceGameObjects;
    fn gameobject_guids(&mut self) -> &mut MapGuidAllocators;
    fn gameobject_spawn(
        &mut self,
        attempt: GridLoadAttempt,
        row: D::Row,
    ) -> Result<GameObjectSpawn>;
    /// Object.cpp:2175-2180 lookup and spawnMask/Map::CanSpawn gate, before construction.
    fn gameobject_dynamic_spawn_allowed(&mut self, _database: u32, _row: &D::Row) -> Result<bool> {
        Err(refuse("execution.gameobject.dynamic_spawn_gate_owner"))
    }
    fn gameobject_template(&mut self, entry: u32) -> Result<Option<GameObjectTemplate>>;
    fn gameobject_thread(&self) -> u64;
    fn gameobject_event_guid(&mut self, guid: u32) -> Result<bool>;
    fn gameobject_force_dynamic(&mut self) -> Result<bool>;
    fn gameobject_group(&mut self, guid: u32) -> Result<Option<GameObjectGroupEntry>>;
    fn gameobject_group_entry(&mut self, group: u32, guid: u32) -> Result<u32>;
    fn gameobject_random_entries(&mut self, guid: u32) -> Result<Option<Arc<[u32]>>>;
    fn gameobject_irand(&mut self, min: i32, max: i32) -> Result<i32>;
    fn gameobject_urand(&mut self, min: u32, max: u32) -> Result<u32>;
    fn gameobject_saved_respawn(&mut self, guid: u32) -> Result<i64>;
    fn gameobject_save_respawn(&mut self, guid: u32, time: i64) -> Result<()>;
    fn gameobject_calendar(&mut self) -> Result<i64>;
    fn gameobject_rotation(&mut self, orientation_bits: u32) -> Result<[u32; 2]>;
    fn gameobject_model(
        &mut self,
        _handle: ObjectHandle,
        _operation: GameObjectModelOperation,
        _context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<bool> {
        Err(GameObjectLifecyclePending::DynamicModel.refusal())
    }
    fn gameobject_callback(
        &mut self,
        handle: ObjectHandle,
        callback: GameObjectCallback,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    // Elevator Create is virtual and initializes attachment state; Transports.cpp:442-467.
    fn gameobject_elevator(
        &mut self,
        _attempt: GridLoadAttempt,
        _spawn: GameObjectSpawn,
        _context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<GridCandidateOutcome> {
        Err(refuse("execution.elevator_constructor_owner_unavailable"))
    }
}

// Spawn lists draw signed irand even for one entry, and a zero result does not override; ObjectMgr.h:600-604, GameObject.cpp:907-908.
fn effective_entry<D, P, F>(
    factory: &mut F,
    spawn: &GameObjectSpawn,
    group: Option<GameObjectGroupEntry>,
    forced: u32,
) -> Result<u32>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
{
    let mut entry = if forced != 0 { forced } else { spawn.entry };
    if let Some(group) = group.filter(|_| entry == 0) {
        entry = factory.gameobject_group_entry(group.id, spawn.database_guid)?;
    }
    if let Some(entries) = factory.gameobject_random_entries(spawn.database_guid)? {
        require(
            !entries.is_empty() && entries.len() <= i32::MAX as usize,
            "execution.gameobject_random_entry_domain",
        )?;
        let index = factory.gameobject_irand(0, entries.len() as i32 - 1)?;
        let random = *entries
            .get(index as usize)
            .ok_or_else(|| refuse("execution.gameobject_random_index"))?;
        if random != 0 {
            entry = random;
        }
    }
    Ok(entry)
}
// Create validates coordinates before template/_Create and type after GUID assignment; GameObject.cpp:178-207.
fn create<D, P, F>(
    factory: &mut F,
    handle: ObjectHandle,
    spawn: &GameObjectSpawn,
    entry: u32,
    counter: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<bool>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
{
    context
        .objects
        .bind_map(handle, context.cells.map(), context.cells)?;
    let position = spawn.position_bits.map(f32::from_bits);
    let half_size = (f32::from_bits(0x4405_5555) * 64.0_f32) / 2.0_f32;
    if !position.iter().all(|value| value.is_finite())
        || position[0].abs() as f64 > half_size as f64 - 0.5
        || position[1].abs() as f64 > half_size as f64 - 0.5
    {
        return Ok(false);
    }
    let Some(template) = factory.gameobject_template(entry)? else {
        return Ok(false);
    };
    context.objects.assign_guid(
        handle,
        spawn.database_guid,
        SourceGuid::with_entry(HighGuid::GameObject, template.entry, counter),
    )?;
    context
        .objects
        .set_pose(handle, SourcePose(spawn.position_bits))?;
    if template.kind >= 31 {
        return Ok(false);
    }
    let mut rotation = spawn.rotation_bits;
    if f32::from_bits(rotation[2]) == 0.0 && f32::from_bits(rotation[3]) == 0.0 {
        [rotation[2], rotation[3]] = factory.gameobject_rotation(spawn.position_bits[3])?;
    }
    let flags = template.flags;
    let display = template.display;
    factory.gameobjects().objects.insert(
        handle,
        SourceGameObject {
            handle,
            template,
            display,
            position_bits: spawn.position_bits,
            rotation_bits: rotation,
            animation: spawn.animation as u8,
            state: 0,
            loot_state: 1,
            flags,
            dynamic_flags: 0,
            level: 0,
            stealth_strength: 0,
            invisibility_value: 0,
            spawned_by_default: true,
            respawn_delay: 25,
            respawn_time: 0,
            linked_trap: None,
            spell: 0,
            owner: SourceGuid::default(),
            spawn: None,
            group: None,
            respawn_overridden: false,
            respawn_override_once: false,
            forced_despawn: false,
            has_loot: false,
            corpse_despawn_ms: i64::MAX,
            restock_time: 0,
            cooldown_time: 0,
            rearm_time: 0,
            capture_timer: 0,
            delayed_action_timer: 0,
            use_times: 0,
            first_user: SourceGuid::default(),
            unique_users: std::collections::BTreeSet::new(),
            skillup_users: std::collections::BTreeSet::new(),
            lifecycle_stopped: false,
            has_model: false,
        },
    );
    // SetDisplayId constructs/inserts the model before initial SetGoState; GameObject.cpp:230-232,2083-2105.
    let model = factory.gameobject_model(
        handle,
        GameObjectModelOperation::ConstructAndInsert,
        context,
    )?;
    factory.gameobjects().get_mut(handle)?.has_model = model;
    let object = factory.gameobjects().get_mut(handle)?;
    object.state = 1;
    let data = &object.template.data;
    match object.template.kind {
        0 | 1 => {
            object.state = u8::from(data[0] == 0);
            if data[0] != 0 {
                object.loot_state = 2;
            }
        }
        6 => {
            object.stealth_strength = if data[9] != 0 { 70 } else { 0 };
            object.invisibility_value = if data[11] != 0 { 300 } else { 0 };
            object.loot_state = 0;
        }
        17 => object.loot_state = 0,
        11 => {
            object.flags |= 0x28;
            object.level = data[0];
            object.state = u8::from(data[1] == 0);
        }
        5 | 8 | 10 | 3 => object.dynamic_flags = 9,
        2 => object.dynamic_flags = 1,
        30 => object.state = u8::from(data[0] == 0),
        _ => {}
    }
    let string = object.template.string_id;
    if string != 0 {
        factory.gameobject_callback(handle, GameObjectCallback::TemplateString(string), context)?;
    }
    factory.gameobject_callback(
        handle,
        GameObjectCallback::CreateBattlegroundOrOutdoor,
        context,
    )?;
    factory.gameobject_callback(handle, GameObjectCallback::CreateInstance, context)?;
    factory.gameobject_callback(handle, GameObjectCallback::LargeVisibility, context)?;
    Ok(true)
}
// A failed LoadFromDB candidate is deleted by LoadHelper, not made resident; ObjectGridLoader.cpp:138-141.
fn failed<D, P, F>(
    factory: &mut F,
    handle: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
{
    if factory.gameobjects().objects.contains_key(&handle) {
        factory.gameobject_model(handle, GameObjectModelOperation::Destroy, context)?;
        factory.gameobjects().objects.remove(&handle);
    }
    context.objects.destroy(handle, context.cells)?;
    Ok(GridCandidateOutcome::FailedAndDestroyed(handle))
}
pub(super) fn add_source_gameobject<D, P, F>(
    factory: &mut F,
    handle: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
{
    let position = factory.gameobjects().get(handle)?.position_bits;
    context.objects.set_pose(handle, SourcePose(position))?;
    let pair = benilla_sim::resident_map::compute_cell_pair(position[0], position[1])
        .map_err(|_| refuse("execution.cell_coordinates"))?;
    let trigger = if context.objects.get(handle)?.active {
        GridLoadTrigger::Enter
    } else {
        GridLoadTrigger::TerrainOnly
    };
    context.load_grid(pair, trigger, factory)?;
    let cell = context.cells.cell(pair)?;
    context.objects.link_cell(handle, cell, context.cells)?;
    context.objects.register_world_lookup(handle)?;
    if factory.gameobjects().get(handle)?.has_model {
        factory.gameobject_model(handle, GameObjectModelOperation::Insert, context)?;
    }
    context.objects.mark_in_world(handle)?;
    let object = factory.gameobjects().get(handle)?;
    let enabled = object.collision_enabled();
    let has_model = object.has_model;
    let linked = if object.is_spawned() {
        object.template.linked_entry()
    } else {
        0
    };
    if has_model {
        factory.gameobject_model(handle, GameObjectModelOperation::Enable(enabled), context)?;
    }
    if linked != 0 {
        let trap = context
            .objects
            .construct(factory.gameobject_thread(), SourceObjectKind::GameObject)?;
        let allocation = factory
            .gameobject_guids()
            .allocate(context.cells.map(), HighGuid::GameObject)?;
        require(!allocation.stop_now, "execution.guid_stop_now")?;
        let spawn = GameObjectSpawn {
            database_guid: allocation.counter,
            entry: linked,
            position_bits: position,
            rotation_bits: [0; 4],
            animation: 100,
            state: -1,
            respawn_min: 0,
            respawn_max: 0,
        };
        if create(factory, trap, &spawn, linked, allocation.counter, context)? {
            let parent = factory.gameobjects().get(handle)?;
            let (spell, owner, level) = (parent.spell, parent.owner, parent.level);
            let child = factory.gameobjects().get_mut(trap)?;
            child.respawn_delay = 0;
            child.set_spell_id(spell);
            if owner.raw() != 0 {
                child.set_owner_guid(owner);
                child.level = level;
            }
            add_source_gameobject(factory, trap, context)?;
            factory.gameobject_callback(trap, GameObjectCallback::InitializeAi, context)?;
            factory.gameobjects().get_mut(handle)?.linked_trap = context.objects.get(trap)?.guid;
        } else {
            failed(factory, trap, context)?;
        }
    }
    if factory.gameobjects().get(handle)?.template.extra_flags & 0x1000 != 0 {
        context.set_active_object_state(handle, true, factory)?;
    }
    if context.objects.get(handle)?.active {
        context.add_to_active(handle, factory)?;
    }
    factory.gameobject_callback(handle, GameObjectCallback::MapViewpoint, context)?;
    factory.gameobject_callback(handle, GameObjectCallback::MapVisibility, context)
}

pub fn load_source_gameobject<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
{
    load_source_gameobject_inner(factory, attempt, row, false, 0, context)
}

/// Object.cpp:2173-2188 enters LoadFromDB with requested GUID zero.
pub fn spawn_source_gameobject<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    forced_entry: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<Option<GridCandidateOutcome>>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
{
    require(
        attempt.kind == GridSpawnKind::GameObject,
        "execution.gameobject_factory_kind",
    )?;
    if !factory.gameobject_dynamic_spawn_allowed(attempt.guid, &row)? {
        return Ok(None);
    }
    load_source_gameobject_inner(factory, attempt, row, true, forced_entry, context).map(Some)
}

fn load_source_gameobject_inner<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    dynamic: bool,
    forced_entry: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
{
    require(
        attempt.kind == GridSpawnKind::GameObject,
        "execution.gameobject_factory_kind",
    )?;
    let spawn = factory.gameobject_spawn(attempt, row)?;
    require(
        spawn.database_guid == attempt.guid,
        "execution.gameobject_spawn_binding",
    )?;
    let initial = factory.gameobject_template(if forced_entry != 0 {
        forced_entry
    } else {
        spawn.entry
    })?;
    if source_gameobject_factory(initial.as_ref()) == GameObjectFactoryKind::Elevator {
        if dynamic {
            return Err(refuse(
                "execution.elevator_dynamic_constructor_owner_unavailable",
            ));
        }
        return factory.gameobject_elevator(attempt, spawn, context);
    }
    let handle = context
        .objects
        .construct(factory.gameobject_thread(), SourceObjectKind::GameObject)?;
    let requested = if dynamic || factory.gameobject_event_guid(spawn.database_guid)? {
        0
    } else {
        spawn.database_guid
    };
    if !context.objects.candidate_allowed(
        context.cells.map(),
        DatabaseLookupKind::GameObject,
        spawn.database_guid,
    ) {
        return failed(factory, handle, context);
    }
    let group = factory.gameobject_group(spawn.database_guid)?;
    let entry = effective_entry::<D, P, F>(factory, &spawn, group, forced_entry)?;
    let force_dynamic = factory.gameobject_force_dynamic()?;
    let extra_dynamic = if force_dynamic {
        false
    } else {
        factory
            .gameobject_template(entry)?
            .is_some_and(|template| template.extra_flags & 2 != 0)
    };
    let allocation = factory.gameobject_guids().spawn_counter(SpawnGuidRequest {
        high: HighGuid::GameObject,
        database: spawn.database_guid,
        requested,
        force_dynamic,
        extra_dynamic,
        group: group.is_some(),
    })?;
    require(!allocation.stop_now, "execution.guid_stop_now")?;
    if !create(factory, handle, &spawn, entry, allocation.counter, context)? {
        return failed(factory, handle, context);
    }
    if spawn.state != -1 {
        factory.gameobjects().get_mut(handle)?.state = spawn.state as u8;
    }
    if let Some(group) = group {
        factory.gameobjects().get_mut(handle)?.group = Some(group);
        factory.gameobject_callback(handle, GameObjectCallback::BindGroup(group.id), context)?;
        if group.string_id != 0 {
            factory.gameobject_callback(
                handle,
                GameObjectCallback::GroupString(group.string_id),
                context,
            )?;
        }
    }
    // Negative respawn never draws; permanent defaults bypass saved respawn; GameObject.cpp:935-963.
    if spawn.respawn_min >= 0 && factory.gameobjects().get(handle)?.template.permanent() {
        let object = factory.gameobjects().get_mut(handle)?;
        object.flags |= 0x20;
        object.respawn_delay = 0;
    } else if spawn.respawn_min >= 0 {
        let delay = factory.gameobject_urand(spawn.respawn_min as u32, spawn.respawn_max as u32)?;
        let mut saved = factory.gameobject_saved_respawn(spawn.database_guid)?;
        if saved != 0 && saved <= factory.gameobject_calendar()? {
            saved = 0;
            factory.gameobject_save_respawn(spawn.database_guid, 0)?;
        }
        let object = factory.gameobjects().get_mut(handle)?;
        object.respawn_delay = delay;
        object.respawn_time = saved;
    } else {
        let delay = spawn
            .respawn_min
            .checked_neg()
            .ok_or_else(|| refuse("execution.gameobject_respawn_signed_overflow"))?;
        let object = factory.gameobjects().get_mut(handle)?;
        object.spawned_by_default = false;
        object.respawn_delay = delay as u32;
    }
    context
        .objects
        .set_respawn_definition(handle, SourcePose(spawn.position_bits), true)?;
    factory.gameobjects().get_mut(handle)?.spawn = Some(spawn);
    add_source_gameobject(factory, handle, context)?;
    factory.gameobject_callback(handle, GameObjectCallback::InitializeAi, context)?;
    Ok(GridCandidateOutcome::Loaded(handle))
}

// These constructors retain separate unavailable owners; Pet.cpp:2108-2137, DynamicObject.cpp:59-103, Transports.cpp:442-467.
pub fn refuse_unavailable_object_constructor(kind: SourceObjectKind) -> Refusal {
    refuse(match kind {
        SourceObjectKind::Pet => "execution.pet_constructor_owner_unavailable",
        SourceObjectKind::DynamicObject => "execution.dynamic_object_constructor_owner_unavailable",
        SourceObjectKind::Transport => "execution.transport_attachment_owner_unavailable",
        _ => "execution.object_constructor_owner_unavailable",
    })
}
