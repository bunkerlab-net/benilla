// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature LoadFromDB executes at the grid factory, with mandatory reached owners.
use super::*;
use benilla_sim::creature_init::DisplayChoice;
use benilla_sim::resident_map::compute_cell_pair;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};

#[derive(Clone, Debug)]
pub struct CreatureSpawn {
    pub database_guid: u32,
    pub entry: u32,
    pub position: [f32; 4],
    pub model: u32,
    pub respawn_seconds: [u32; 2],
    pub respawn_radius: f32,
    pub relay: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CreatureEventData {
    pub entry: u32,
    pub model: u32,
    pub equipment: u32,
}
#[derive(Clone, Debug)]
pub struct CreatureTemplate {
    pub entry: u32,
    pub dynamic_guid: bool,
    pub active: bool,
    pub count_spawns: bool,
    pub displays: [DisplayChoice; 4],
    pub levels: [u32; 2],
    pub corpse_seconds: u32,
    pub can_fly: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureModel {
    pub id: u32,
    pub gender: u8,
    pub other_gender: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureGroup {
    pub id: u32,
    pub string_id: u32,
    pub has_formation: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureEntryOwner {
    Group(CreatureGroup),
    Conditional,
    Random { lookup_database_guid: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreaturePhase {
    InitEntryBase,
    ModelAndDisplay,
    SetDeadAndZeroHealth,
    InitEntryEquipmentAndFlags,
    SelectLevelStats,
    UpdateEntryFlagsAndSpells,
    SelectFinalPointAndRelocate,
    PvpAndInstanceCreate,
    LinkingRegistration,
    LoadAddon,
    AttachGroup,
    MapAddToWorld,
    AddToSpawnCount,
    MapAddViewpoint,
    MapAddVisibility,
    MotionInitialize,
    SelectAi,
    AiSpellListChanged,
    AiJustRespawned,
    InstanceRespawnAndCreatorLoot,
    Relay(u32),
    LinkingRespawn,
    FormationSlot,
    GroupRespawn,
    RabbitDay,
}
impl CreaturePhase {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::InitEntryBase
            | Self::ModelAndDisplay
            | Self::InitEntryEquipmentAndFlags
            | Self::SelectLevelStats
            | Self::UpdateEntryFlagsAndSpells => "execution.creature.unit_initialization_owner",
            Self::SetDeadAndZeroHealth => "execution.creature.dead_health_owner",
            Self::SelectFinalPointAndRelocate => "execution.creature.geometry_position_owner",
            Self::PvpAndInstanceCreate => "execution.creature.create_script_owner",
            Self::LinkingRegistration | Self::LinkingRespawn => "execution.creature.linking_owner",
            Self::LoadAddon => "execution.creature.addon_owner",
            Self::AttachGroup | Self::FormationSlot | Self::GroupRespawn => {
                "execution.creature.group_owner"
            }
            Self::MapAddToWorld => "execution.creature.add_to_world_owner",
            Self::AddToSpawnCount => "execution.creature.spawn_count_owner",
            Self::MapAddViewpoint => "execution.creature.viewpoint_owner",
            Self::MapAddVisibility => "execution.creature.visibility_owner",
            Self::MotionInitialize => "execution.creature.motion_owner",
            Self::SelectAi | Self::AiSpellListChanged | Self::AiJustRespawned => {
                "execution.creature.ai_owner"
            }
            Self::InstanceRespawnAndCreatorLoot => "execution.creature.instance_respawn_owner",
            Self::Relay(_) => "execution.creature.relay_owner",
            Self::RabbitDay => "execution.creature.rabbit_calendar_spell_owner",
        })
    }
}
#[derive(Clone, Debug)]
pub struct CreatureConstruction {
    pub object: ObjectHandle,
    pub spawn: CreatureSpawn,
    pub original_entry: u32,
    pub template: CreatureTemplate,
    pub event: Option<CreatureEventData>,
    pub group: Option<CreatureGroup>,
    pub position: [f32; 4],
    pub respawn_position: [f32; 4],
    pub model: Option<CreatureModel>,
    pub rolls: benilla_sim::creature_init::SelectionRolls,
    pub level: u32,
    pub respawn_delay: u32,
    pub corpse_delay: u32,
    pub respawn_time: i64,
    pub linking_trigger: bool,
    pub spawning_linked: bool,
}
impl CreatureConstruction {
    /// Retain the constructed object across lives; Creature.cpp:717-750,1912-1944.
    pub fn into_lifecycle(
        self,
        settings: CreatureLifecycleSettings,
        objects: &SourceObjects,
    ) -> Result<SourceCreatureLifecycle> {
        SourceCreatureLifecycle::new(self, settings, objects)
    }
}
/// Borrow the calling engine without result inputs; Util.cpp:68-78.
pub trait CreatureRandom {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32>;
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32>;
}
impl<E: RawGenerator, S: FnMut(TraceEvent)> CreatureRandom for SourceRandom<'_, E, S> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.irand(min, max)
            .map_err(|_| refuse("execution.creature.irand"))
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.urand(min, max)
            .map_err(|_| refuse("execution.creature.urand"))
    }
}
/// Execute reached owners through the reentrant factory; Creature.cpp:898-934,1738-1765.
pub trait CreatureFactoryOwner<D: GridDefinitions, P: GridProductOwner>:
    GridObjectFactory<D, P>
{
    fn creature_thread(&self) -> u64;
    fn creature_construct_candidate(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn creature_spawn(
        &mut self,
        attempt: GridLoadAttempt,
        row: D::Row,
    ) -> Result<Option<CreatureSpawn>>;
    /// Object.cpp:2193-2203 lookup and spawnMask/Map::CanSpawn gate, before construction.
    fn creature_dynamic_spawn_allowed(&mut self, _database: u32, _row: &D::Row) -> Result<bool> {
        Err(refuse("execution.creature.dynamic_spawn_gate_owner"))
    }
    fn creature_event_guid(&mut self, database: u32) -> Result<bool>;
    fn creature_force_dynamic(&mut self) -> Result<bool>;
    fn creature_force_load_map(&mut self) -> Result<bool>;
    fn creature_group(&mut self, database: u32) -> Result<Option<CreatureGroup>>;
    fn creature_entry(&mut self, database: u32, owner: CreatureEntryOwner) -> Result<u32>;
    fn creature_event(&mut self, database: u32) -> Result<Option<CreatureEventData>>;
    fn creature_template(&mut self, entry: u32) -> Result<Option<CreatureTemplate>>;
    fn creature_model(&mut self, display: u32) -> Result<Option<CreatureModel>>;
    fn creature_guids(&mut self) -> &mut MapGuidAllocators;
    fn creature_random(&mut self) -> Result<&mut dyn CreatureRandom>;
    fn creature_respawn_time(&mut self, database: u32) -> Result<i64>;
    fn creature_save_respawn_time(&mut self, database: u32, time: i64) -> Result<()>;
    fn creature_wall_time(&mut self, object: ObjectHandle) -> Result<ClockSample>;
    fn creature_spawned_by_linked_mob(&mut self, state: &CreatureConstruction) -> Result<bool>;
    fn creature_linked_can_spawn(&mut self, state: &CreatureConstruction) -> Result<bool>;
    fn creature_dead_flying_height(&mut self, state: &CreatureConstruction) -> Result<f32>;
    fn creature_phase(
        &mut self,
        phase: CreaturePhase,
        state: &mut CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn creature_created(
        &mut self,
        state: CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn creature_destroy_candidate(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
}
fn failed<D, P, F>(
    factory: &mut F,
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    factory.creature_destroy_candidate(object, context)?;
    context.objects.destroy(object, context.cells)?;
    Ok(GridCandidateOutcome::FailedAndDestroyed(object))
}
// Construction precedes LoadFromDB, including duplicate skips; ObjectGridLoader.cpp:133-140.
pub fn load_source_creature<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    load_source_creature_inner(factory, attempt, row, None, 0, context)
}

/// Object.cpp:2191-2212 enters LoadFromDB with requested GUID zero, not the grid-loader GUID.
pub fn spawn_source_creature<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    forced_entry: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<Option<GridCandidateOutcome>>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    require(
        attempt.kind == GridSpawnKind::Creature,
        "execution.creature.factory_kind",
    )?;
    if !factory.creature_dynamic_spawn_allowed(attempt.guid, &row)? {
        return Ok(None);
    }
    load_source_creature_inner(factory, attempt, row, Some(0), forced_entry, context).map(Some)
}

/// Pool leaves pass their database GUID directly; PoolManager.cpp:452-454.
pub fn load_pool_source_creature<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    load_source_creature_inner(factory, attempt, row, Some(attempt.guid), 0, context)
}

fn load_source_creature_inner<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    requested_guid: Option<u32>,
    forced_entry: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    require(
        attempt.kind == GridSpawnKind::Creature,
        "execution.creature.factory_kind",
    )?;
    let object = context
        .objects
        .construct(factory.creature_thread(), SourceObjectKind::Creature)?;
    factory.creature_construct_candidate(object, context)?;
    // An unsupported owner is a stopped execution, not a source LoadFromDB false/delete.
    let requested = if let Some(guid) = requested_guid {
        guid
    } else if factory.creature_event_guid(attempt.guid)? {
        0
    } else {
        attempt.guid
    };
    let Some(spawn) = factory.creature_spawn(attempt, row)? else {
        return failed(factory, object, context);
    };
    require(
        spawn.database_guid == attempt.guid,
        "execution.creature.definition_binding",
    )?;
    if !context.objects.candidate_allowed(
        attempt.grid.map,
        DatabaseLookupKind::Creature,
        attempt.guid,
    ) {
        return failed(factory, object, context);
    }
    let group = factory.creature_group(attempt.guid)?;
    let mut entry = if forced_entry != 0 {
        forced_entry
    } else {
        spawn.entry
    };
    // Group, conditional, random and event fallback are ordered; Creature.cpp:1630-1653.
    if entry == 0 {
        if let Some(group) = group {
            entry = factory.creature_entry(attempt.guid, CreatureEntryOwner::Group(group))?;
        }
    }
    if entry == 0 {
        entry = factory.creature_entry(attempt.guid, CreatureEntryOwner::Conditional)?;
    }
    // GetDbGuid is still zero before _Create, not the LoadFromDB argument; Creature.cpp:1646,1608.
    if entry == 0 {
        entry = factory.creature_entry(
            attempt.guid,
            CreatureEntryOwner::Random {
                lookup_database_guid: 0,
            },
        )?;
    }
    let event = factory.creature_event(attempt.guid)?;
    if entry == 0 {
        entry = event.map_or(0, |event| event.entry);
    }
    if entry == 0 {
        return failed(factory, object, context);
    }
    let Some(original) = factory.creature_template(entry)? else {
        return failed(factory, object, context);
    };
    require(
        original.entry == entry,
        "execution.creature.template_binding",
    )?;
    let force_dynamic = factory.creature_force_dynamic()?;
    require(
        factory.creature_guids().map() == attempt.grid.map,
        "execution.creature.allocator_map",
    )?;
    let allocation = factory.creature_guids().spawn_counter(SpawnGuidRequest {
        high: HighGuid::Creature,
        database: attempt.guid,
        requested,
        force_dynamic,
        extra_dynamic: original.dynamic_guid,
        group: group.is_some(),
    })?;
    require(!allocation.stop_now, "execution.creature.guid_stop_now")?;
    context
        .objects
        .bind_map(object, attempt.grid.map, context.cells)?;
    context
        .objects
        .set_pose(object, SourcePose(spawn.position.map(f32::to_bits)))?;
    context.objects.set_respawn_definition(
        object,
        SourcePose(spawn.position.map(f32::to_bits)),
        true,
    )?;
    // GUID uses the original entry before event InitEntry override; Creature.cpp:1604-1610,322-324.
    context.objects.assign_guid(
        object,
        attempt.guid,
        SourceGuid::with_entry(HighGuid::Creature, entry, allocation.counter),
    )?;
    let effective = event.filter(|e| e.entry != 0).map_or(entry, |e| e.entry);
    let template = if effective == entry {
        original
    } else {
        let Some(template) = factory.creature_template(effective)? else {
            return failed(factory, object, context);
        };
        require(
            template.entry == effective,
            "execution.creature.template_binding",
        )?;
        template
    };
    let corpse_delay = template.corpse_seconds;
    let position = spawn.position;
    let mut state = CreatureConstruction {
        object,
        spawn,
        original_entry: entry,
        template,
        event,
        group,
        position,
        respawn_position: position,
        model: None,
        level: 0,
        respawn_delay: 0,
        corpse_delay,
        respawn_time: 0,
        rolls: benilla_sim::creature_init::SelectionRolls {
            model: None,
            gender: None,
            level: None,
        },
        linking_trigger: false,
        spawning_linked: false,
    };
    factory.creature_phase(CreaturePhase::InitEntryBase, &mut state, context)?;
    let mut display = event.map_or(0, |event| event.model);
    if display == 0 {
        display = state.spawn.model;
    }
    if display == 0 {
        let mut total = 0_u32;
        for choice in state.template.displays {
            if choice.id != 0 {
                total = total.wrapping_add(choice.probability);
            }
        }
        // Signed subtraction is undefined only at INT32_MIN; Creature.cpp:678.
        let upper = (total as i32)
            .checked_sub(1)
            .ok_or_else(|| refuse("execution.creature.display_signed_overflow"))?
            .max(0);
        let mut roll = factory.creature_random()?.creature_irand(0, upper)?;
        state.rolls.model = Some(roll as u32);
        for choice in state.template.displays {
            if choice.id != 0 {
                if roll < choice.probability as i32 {
                    display = choice.id;
                    break;
                }
                roll = (roll as u32).wrapping_sub(choice.probability) as i32;
            }
        }
        if display == 0 {
            display = factory
                .creature_template(1)?
                .map_or(0, |t| t.displays[0].id);
        }
    }
    if display == 0 {
        return failed(factory, object, context);
    }
    let Some(mut model) = factory.creature_model(display)? else {
        return failed(factory, object, context);
    };
    if model.other_gender != 0 {
        let roll = factory.creature_random()?.creature_urand(0, 1)?;
        state.rolls.gender = Some(roll);
        if roll == 0 {
            if let Some(other) = factory.creature_model(model.other_gender)? {
                model = other;
            }
        }
    }
    state.model = Some(model);
    factory.creature_phase(CreaturePhase::ModelAndDisplay, &mut state, context)?;
    factory.creature_phase(
        CreaturePhase::InitEntryEquipmentAndFlags,
        &mut state,
        context,
    )?;
    let [min, max] = state.template.levels;
    state.level = if min == max {
        min
    } else {
        factory.creature_random()?.creature_urand(min, max)?
    };
    state.rolls.level = (min != max).then_some(state.level);
    factory.creature_phase(CreaturePhase::SelectLevelStats, &mut state, context)?;
    factory.creature_phase(
        CreaturePhase::UpdateEntryFlagsAndSpells,
        &mut state,
        context,
    )?;
    factory.creature_phase(
        CreaturePhase::SelectFinalPointAndRelocate,
        &mut state,
        context,
    )?;
    if !valid_position(state.position) {
        return failed(factory, object, context);
    }
    context
        .objects
        .set_pose(object, SourcePose(state.position.map(f32::to_bits)))?;
    for phase in [
        CreaturePhase::PvpAndInstanceCreate,
        CreaturePhase::LinkingRegistration,
        CreaturePhase::LoadAddon,
    ] {
        factory.creature_phase(phase, &mut state, context)?;
    }
    if group.is_some() {
        factory.creature_phase(CreaturePhase::AttachGroup, &mut state, context)?;
    }
    let [min, max] = state.spawn.respawn_seconds;
    state.respawn_delay = factory.creature_random()?.creature_urand(min, max)?;
    let runtime_counter = context
        .objects
        .get(object)?
        .guid
        .ok_or_else(|| refuse("execution.creature.guid_absent"))?
        .counter();
    let new_spawning = source_creature_new_spawning(attempt.guid, runtime_counter);
    if !new_spawning {
        state.corpse_delay = state
            .corpse_delay
            .min(state.respawn_delay.wrapping_mul(9) / 10);
    }
    state.respawn_time = factory.creature_respawn_time(attempt.guid)?;
    let now = factory.creature_wall_time(object)?;
    require(
        now.kind() == ClockKind::WallSeconds,
        "execution.creature.respawn_clock",
    )?;
    if state.respawn_time > now.value() {
        if new_spawning && group.is_none() {
            return failed(factory, object, context);
        }
        dead(factory, &mut state, context)?;
    } else if state.respawn_time != 0 {
        state.respawn_time = 0;
        factory.creature_save_respawn_time(attempt.guid, 0)?;
    }
    state.spawning_linked = factory.creature_spawned_by_linked_mob(&state)?;
    if state.spawning_linked
        && context.objects.get(object)?.alive
        && !factory.creature_linked_can_spawn(&state)?
    {
        dead(factory, &mut state, context)?;
    }
    add_source_creature(factory, &mut state, context)?;
    for phase in [
        CreaturePhase::MotionInitialize,
        CreaturePhase::SelectAi,
        CreaturePhase::AiSpellListChanged,
        CreaturePhase::AiJustRespawned,
        CreaturePhase::InstanceRespawnAndCreatorLoot,
    ] {
        factory.creature_phase(phase, &mut state, context)?;
    }
    if state.spawn.relay != 0 {
        factory.creature_phase(CreaturePhase::Relay(state.spawn.relay), &mut state, context)?;
    }
    if state.linking_trigger && context.objects.get(object)?.alive {
        factory.creature_phase(CreaturePhase::LinkingRespawn, &mut state, context)?;
    }
    if let Some(group) = state.group {
        if group.has_formation {
            factory.creature_phase(CreaturePhase::FormationSlot, &mut state, context)?;
        }
        factory.creature_phase(CreaturePhase::GroupRespawn, &mut state, context)?;
    }
    if context.objects.get(object)?.alive {
        factory.creature_phase(CreaturePhase::RabbitDay, &mut state, context)?;
    }
    factory.creature_created(state, context)?;
    Ok(GridCandidateOutcome::Loaded(object))
}

/// Re-add a retained construction without LoadFromDB or AI initialization; Map.cpp:471-504, Creature.cpp:173-209.
pub fn add_source_creature<D, P, F>(
    factory: &mut F,
    state: &mut CreatureConstruction,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    let object = state.object;
    state.position = context.objects.pose(object)?.0.map(f32::from_bits);
    let cell = compute_cell_pair(state.position[0].to_bits(), state.position[1].to_bits())
        .map_err(|_| refuse("execution.creature.map_add_coordinates"))?;
    if context.objects.get(object)?.active {
        context.load_grid(cell, GridLoadTrigger::Enter, factory)?;
    } else {
        context.load_grid(cell, GridLoadTrigger::TerrainOnly, factory)?;
    }
    let cell = context.cells.cell(cell)?;
    context.objects.link_cell(object, cell, context.cells)?;
    context.objects.add_to_world(object)?;
    factory.creature_phase(CreaturePhase::MapAddToWorld, state, context)?;
    if factory.creature_force_load_map()? || state.template.active {
        context.set_active_object_state(object, true, factory)?;
    }
    if state.template.count_spawns {
        factory.creature_phase(CreaturePhase::AddToSpawnCount, state, context)?;
    }
    if context.objects.get(object)?.active {
        context.add_to_active(object, factory)?;
    }
    factory.creature_phase(CreaturePhase::MapAddViewpoint, state, context)?;
    factory.creature_phase(CreaturePhase::MapAddVisibility, state, context)
}
fn valid_position(position: [f32; 4]) -> bool {
    // Position validity has a half-yard margin, unlike cell conversion; GridDefines.h:177-194.
    const LIMIT: f64 = (533.333_3_f32 * 64.0 / 2.0) as f64 - 0.5;
    position.iter().all(|v| v.is_finite())
        && f64::from(position[0].abs()) <= LIMIT
        && f64::from(position[1].abs()) <= LIMIT
}
fn dead<D, P, F>(
    factory: &mut F,
    state: &mut CreatureConstruction,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    context.objects.mark_dead(state.object)?;
    factory.creature_phase(CreaturePhase::SetDeadAndZeroHealth, state, context)?;
    if state.template.can_fly {
        let height = factory.creature_dead_flying_height(state)?;
        if state.position[2] - height > 0.1 {
            state.position[2] = height;
        }
    }
    context
        .objects
        .set_pose(state.object, SourcePose(state.position.map(f32::to_bits)))?;
    Ok(())
}

/// Resolve arithmetic from consumed rolls at SelectLevelStats; Creature.cpp:523,1273-1279.
pub fn resolve_supported_creature_kernel(
    input: &benilla_sim::creature_init::Input,
    state: &CreatureConstruction,
) -> Result<benilla_sim::creature_init::InitializedCreature> {
    use benilla_sim::creature_init as init;
    require(
        state.event.is_none() && state.group.is_none() && !state.template.can_fly,
        "execution.creature.kernel_domain_owner",
    )?;
    require(
        input.spawn_id == u64::from(state.spawn.database_guid)
            && input.template.entry == state.template.entry
            && input.template.displays == state.template.displays
            && [
                u32::from(input.template.minimum_level),
                u32::from(input.template.maximum_level),
            ] == state.template.levels,
        "execution.creature.kernel_operands",
    )?;
    let init::Rows::Complete(spawns) = &input.spawn_template else {
        return Err(refuse("execution.creature.kernel_spawn_template_owner"));
    };
    require(
        spawns.len() == 1 && spawns[0].model_id == state.spawn.model,
        "execution.creature.kernel_spawn_template_owner",
    )?;
    let value = init::prepare(input)
        .and_then(|plan| plan.resolve(state.rolls))
        .map_err(|error| Refusal {
            id: format!("execution.creature.kernel_owner:{error}"),
        })?;
    let model = state
        .model
        .ok_or_else(|| refuse("execution.creature.kernel_model_prefix"))?;
    require(
        value.model.display_id == model.id
            && value.model.gender == model.gender
            && u32::from(value.level) == state.level,
        "execution.creature.kernel_model_prefix",
    )?;
    Ok(value)
}

/// Read the current entry field, not the entry bits retained in a runtime GUID.
pub fn lookup_source_creature_linking(
    links: &SourceCreatureLinking,
    objects: &SourceObjects,
    object: ObjectHandle,
    entry: u32,
) -> Result<Option<LinkingInfo>> {
    links.ensure_loaded()?;
    let value = objects.get(object)?;
    let map = value
        .map
        .ok_or_else(|| refuse("execution.linking.object_map_kind"))?;
    require(
        matches!(
            value.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ),
        "execution.linking.object_map_kind",
    )?;
    Ok(links.lookup(entry, value.database_guid, map.map))
}

/// Creature.cpp:923-929. The phase owner sets its trigger flag on a true result.
pub fn register_source_creature_linking(
    links: &SourceCreatureLinking,
    holder: &mut SourceCreatureLinkingHolder,
    objects: &SourceObjects,
    object: ObjectHandle,
    entry: u32,
) -> Result<bool> {
    let info = lookup_source_creature_linking(links, objects, object, entry)?;
    let value = objects.get(object)?;
    require(
        value.map == Some(holder.map()),
        "execution.linking.object_map_kind",
    )?;
    if info.is_some() {
        holder.add_slave(links, objects, object, entry)?;
    }
    let trigger = links.event_trigger(entry, value.database_guid, holder.map().map);
    if trigger {
        holder.add_master(links, objects, object, entry)?;
    }
    Ok(trigger)
}
