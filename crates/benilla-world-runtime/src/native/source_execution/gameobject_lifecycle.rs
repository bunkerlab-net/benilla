// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! GameObject lifecycle mutates the constructor's retained fields, not a second actor.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GameObjectLifecyclePending {
    DynamicModel,
    PoolEffects,
    GroupEffects,
    Events,
    Ai,
    Instance,
    Loot,
    WorldState,
    Gameplay,
    Evidence,
}
impl GameObjectLifecyclePending {
    pub fn refusal(self) -> Refusal {
        refuse(match self {
            Self::DynamicModel => "execution.dynamic_collision.gameobject_world_geometry_owner",
            Self::PoolEffects => "execution.gameobject_lifecycle.f4_09_pool_effects_pending",
            Self::GroupEffects => "execution.gameobject_lifecycle.group_dispatch_owner",
            Self::Events => "execution.gameobject_lifecycle.f5_events_pending",
            Self::Ai => "execution.gameobject_lifecycle.f5_ai_pending",
            Self::Instance => "execution.gameobject_lifecycle.f5_instance_pending",
            Self::Loot => "execution.gameobject_lifecycle.f5_loot_pending",
            Self::WorldState => "execution.gameobject_lifecycle.f5_worldstate_pending",
            Self::Gameplay => "execution.gameobject_lifecycle.f5_gameplay_pending",
            Self::Evidence => "execution.gameobject_lifecycle.mr_mp_mpr_evidence_pending",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameObjectLifecyclePhase {
    Events(u32),
    WorldStateRevert,
    DespawnAnimation,
    DestroyLoot,
    ClearLootRecipient,
    AiJustDespawned,
    InstanceDespawn,
    LootStateChange(Option<ObjectHandle>),
    RemoveWildOwner,
    Visibility,
    DelayedAction,
    ScheduleForcedDespawn(u32),
    AiUpdate(u32),
    WorldObjectUpdate(u32),
    FishNotHooked,
    FishReady,
    TrapSearch { radius_bits: u32 },
    GooberCasts,
    CapturePointDeactivate,
    CaptureTick,
    ForceDynamicUpdate,
    LootUpdate,
    PoolRespawn(u32),
}
impl GameObjectLifecyclePhase {
    pub fn missing_owner(self) -> Refusal {
        use GameObjectLifecyclePending as P;
        match self {
            Self::Events(_) | Self::ScheduleForcedDespawn(_) => P::Events,
            Self::AiJustDespawned | Self::AiUpdate(_) | Self::LootStateChange(_) => P::Ai,
            Self::InstanceDespawn => P::Instance,
            Self::DestroyLoot | Self::ClearLootRecipient | Self::LootUpdate => P::Loot,
            Self::WorldStateRevert => P::WorldState,
            Self::PoolRespawn(_) => P::PoolEffects,
            _ => P::Gameplay,
        }
        .refusal()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameObjectLifecycleClock {
    ReadyExpiry,
    Save,
    Deactivate,
    ChestRestock,
    Respawn,
    SetRespawn,
    FishingReady,
    TrapCooldown,
    TrapArm,
    AutoClose,
    TrapRearmStart,
    TrapRearmCheck,
    GooberCooldown,
}
pub struct GameObjectLifecycleContext<'a, 'b, D: GridDefinitions, P: ResidencyProductOwner> {
    pub grid: &'a mut GridFactoryContext<'b, D, P>,
    pub persistent: &'a mut SourceManagers,
    pub spawns: &'a mut SourceSpawnManager,
}
/// Reached F5/F6 effects execute synchronously; there is no successful default handler.
pub trait GameObjectLifecycleOwner<D: GridDefinitions, P: ResidencyProductOwner>:
    GameObjectFactoryOwner<D, P> + ResidencyOwner<D, P> + SpawnWork
{
    fn elevator_map_millis(&mut self, _map: MapIdentity) -> Result<u32> {
        Err(refuse("execution.elevator.map_snapshot_owner"))
    }
    // Elevators and MO transports share Unit attachment state; Transports.h:30-69.
    fn elevator_passengers(&mut self) -> Result<&mut SourcePassengers> {
        Err(refuse("execution.transport.passenger_registry_owner"))
    }
    fn elevator_passenger_effect(
        &mut self,
        effect: PassengerEffect,
        grid: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>
    where
        Self: Sized,
    {
        effect.creature(grid, self)
    }
    fn go_lifecycle_wall(
        &mut self,
        object: ObjectHandle,
        call: GameObjectLifecycleClock,
    ) -> Result<ClockSample>;
    fn go_save_immediately(&mut self) -> Result<bool>;
    fn go_lifecycle_phase(
        &mut self,
        _object: ObjectHandle,
        phase: GameObjectLifecyclePhase,
        _context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
    ) -> Result<()> {
        Err(phase.missing_owner())
    }
    fn go_group_delay(
        &mut self,
        _object: ObjectHandle,
        _context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
    ) -> Result<Option<u32>> {
        Err(GameObjectLifecyclePending::GroupEffects.refusal())
    }
    fn go_pool(&mut self, _database_guid: u32) -> Result<Option<u32>> {
        Err(GameObjectLifecyclePending::PoolEffects.refusal())
    }
    fn go_pending_events(&mut self, _object: ObjectHandle) -> Result<usize> {
        Err(GameObjectLifecyclePending::Events.refusal())
    }
    fn go_owner_in_combat(&mut self, _object: ObjectHandle) -> Result<bool> {
        Err(GameObjectLifecyclePending::Gameplay.refusal())
    }
    fn go_clear_group(
        &mut self,
        _object: ObjectHandle,
        _grid: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(GameObjectLifecyclePending::GroupEffects.refusal())
    }
}
impl SourceGameObject {
    pub fn set_respawn_delay(&mut self, seconds: u32, once: bool) {
        // Only the active flag clears after a one-shot delay; GameObject.h:771, GameObject.cpp:696-697.
        self.respawn_delay = seconds;
        self.respawn_overridden = true;
        self.respawn_override_once = once;
    }
    pub fn respawn_save_state(&self) -> RespawnSaveState {
        RespawnSaveState::GameObject {
            respawn_seconds: self.respawn_time,
            spawned_by_default: self.spawned_by_default,
        }
    }
    fn clear_uses(&mut self) {
        // Reset every use collection, not only charges; GameObject.h:802-807.
        self.skillup_users.clear();
        self.use_times = 0;
        self.first_user = SourceGuid::default();
        self.unique_users.clear();
    }
}
impl GameObjectTemplate {
    fn despawn_at_action(&self) -> bool {
        // Consumable lives in different union members; GameObject.h:377-384.
        match self.kind {
            3 => self.data[3] != 0,
            10 => self.data[5] != 0,
            _ => false,
        }
    }
    fn despawn_possible(&self) -> bool {
        // The default permits despawn, including generic objects; GameObject.h:419-429.
        match self.kind {
            0 => self.data[3] != 0,
            1 => self.data[4] != 0,
            10 => self.data[11] != 0,
            24 => self.data[5] != 0,
            26 => self.data[3] != 0,
            _ => true,
        }
    }
    fn charges(&self) -> i32 {
        // GetCharges returns uint32 before Update assigns int32; GameObject.h:447-455, GameObject.cpp:508.
        match self.kind {
            6 => self.data[4] as i32,
            21 => self.data[1] as i32,
            22 => self.data[1] as i32,
            _ => 0,
        }
    }
}
fn run<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
    action: impl FnOnce(&mut F, &mut GameObjectLifecycleContext<'_, '_, D, P>) -> Result<()>,
) -> Result<()> {
    require(
        !owner.gameobjects().get(object)?.lifecycle_stopped,
        "execution.gameobject_lifecycle.stopped",
    )?;
    require(
        context.grid.objects.get(object)?.map == Some(context.grid.cells.map())
            && context.spawns.map() == context.grid.cells.map()
            && context
                .persistent
                .state(context.spawns.persistent())?
                .map_binding()
                == Some(context.grid.cells.map()),
        "execution.gameobject_lifecycle.map_binding",
    )?;
    let result = action(owner, context);
    if result.is_err() {
        owner.gameobjects().get_mut(object)?.lifecycle_stopped = true;
    }
    result
}
fn wall<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    call: GameObjectLifecycleClock,
) -> Result<i64> {
    let sample = owner.go_lifecycle_wall(object, call)?;
    require(
        sample.kind() == ClockKind::WallSeconds,
        "execution.gameobject_lifecycle.wall_kind",
    )?;
    Ok(sample.value())
}
fn seconds(now: i64, delay: i64) -> Result<i64> {
    now.checked_add(delay)
        .ok_or_else(|| refuse("execution.gameobject_lifecycle.signed_time_overflow_undefined"))
}
fn new_system(object: ObjectHandle, objects: &SourceObjects) -> Result<bool> {
    let value = objects.get(object)?;
    let guid = value
        .guid
        .ok_or_else(|| refuse("execution.object_guid_absent"))?;
    // New spawning compares nonzero DBGUID with runtime low GUID; Object.cpp:3076-3078.
    Ok(value.database_guid != 0 && value.database_guid != guid.counter())
}
/// F6 computes membership and relocation against its actual dynamic model/tree.
pub fn update_source_gameobject_model_position<
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    if owner.gameobjects().get(object)?.has_model
        && owner.gameobject_model(object, GameObjectModelOperation::Contains, grid)?
    {
        // Relocation does not insert a previously absent model; GameObject.cpp:2129-2139.
        owner.gameobject_model(object, GameObjectModelOperation::Remove, grid)?;
        owner.gameobject_model(object, GameObjectModelOperation::Relocate, grid)?;
        owner.gameobject_model(object, GameObjectModelOperation::Insert, grid)?;
    }
    Ok(())
}
pub fn add_source_gameobject_model<
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    if owner.gameobjects().get(object)?.has_model
        && !owner.gameobject_model(object, GameObjectModelOperation::Contains, grid)?
    {
        owner.gameobject_model(object, GameObjectModelOperation::Relocate, grid)?;
        owner.gameobject_model(object, GameObjectModelOperation::Insert, grid)?;
    }
    Ok(())
}
pub fn remove_source_gameobject_model<
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    if owner.gameobjects().get(object)?.has_model
        && owner.gameobject_model(object, GameObjectModelOperation::Contains, grid)?
    {
        owner.gameobject_model(object, GameObjectModelOperation::Remove, grid)?;
    }
    Ok(())
}
pub fn update_source_gameobject_model<
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    let had_model = owner.gameobjects().get(object)?.has_model;
    if had_model && grid.objects.get(object)?.in_world {
        remove_source_gameobject_model(owner, object, grid)?;
    }
    if had_model {
        owner.gameobject_model(object, GameObjectModelOperation::Destroy, grid)?;
    }
    let has_model =
        owner.gameobject_model(object, GameObjectModelOperation::ConstructAndInsert, grid)?;
    owner.gameobjects().get_mut(object)?.has_model = has_model;
    Ok(())
}
fn collision<D: GridDefinitions, P: GridProductOwner, F: GameObjectFactoryOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    let value = owner.gameobjects().get(object)?;
    if value.has_model && grid.objects.get(object)?.in_world {
        let enabled = value.collision_enabled();
        owner.gameobject_model(object, GameObjectModelOperation::Enable(enabled), grid)?;
    }
    Ok(())
}
fn loot<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    state: u8,
    user: Option<ObjectHandle>,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    owner.gameobjects().get_mut(object)?.loot_state = state;
    collision(owner, object, context.grid)?;
    owner.go_lifecycle_phase(
        object,
        GameObjectLifecyclePhase::LootStateChange(user),
        context,
    )
}
fn state<D: GridDefinitions, P: GridProductOwner, F: GameObjectFactoryOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    state: u8,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    owner.gameobjects().get_mut(object)?.state = state;
    collision(owner, object, grid)
}
pub fn set_source_gameobject_loot_state<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    state: u8,
    user: Option<ObjectHandle>,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| {
        loot(o, object, state, user, c)
    })
}
pub fn set_source_gameobject_state<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    value: u8,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| {
        state(o, object, value, c.grid)
    })
}
fn save<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    let now = wall(owner, object, GameObjectLifecycleClock::Save)?;
    let value = owner.gameobjects().get(object)?;
    if value.respawn_time > now && value.spawned_by_default {
        let deadline = value.respawn_time;
        let database = context.grid.objects.get(object)?.database_guid;
        context.persistent.save_respawn(
            context.spawns.persistent(),
            GridSpawnKind::GameObject,
            database,
            deadline,
            owner.world_game_seconds()?,
            owner,
        )?;
    }
    Ok(())
}
pub fn save_source_gameobject_respawn<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| save(o, object, c))
}
fn linked<F: GameObjectFactoryOwner<D, P>, D: GridDefinitions, P: GridProductOwner>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &GridFactoryContext<'_, D, P>,
) -> Result<Option<ObjectHandle>> {
    Ok(owner
        .gameobjects()
        .get(object)?
        .linked_trap
        .and_then(|guid| grid.objects.lookup_world_object(grid.cells.map(), guid)))
}
fn delete_inner<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    use GameObjectLifecyclePhase as Phase;
    owner.go_lifecycle_phase(object, Phase::DespawnAnimation, context)?;
    state(owner, object, 1, context.grid)?;
    let flags = owner.gameobjects().get(object)?.template.flags;
    owner.gameobjects().get_mut(object)?.flags = flags;
    owner.go_lifecycle_phase(object, Phase::AiJustDespawned, context)?;
    owner.go_lifecycle_phase(object, Phase::InstanceDespawn, context)?;
    let database = context.grid.objects.get(object)?.database_guid;
    if let Some(pool) = owner.go_pool(database)? {
        owner.go_lifecycle_phase(object, Phase::PoolRespawn(pool), context)?;
    } else {
        context.grid.add_to_remove(object, owner)?;
    }
    if let Some(trap) = linked(owner, object, context.grid)? {
        loot(owner, trap, 3, None, context)?;
        delete_inner(owner, trap, context)?;
    }
    Ok(())
}
pub fn delete_source_gameobject<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| delete_inner(o, object, c))
}
/// Called by F3.11's RemoveFromWorld hook before the registry's base removal.
pub fn source_gameobject_remove_world_effects<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    if !context.objects.get(object)?.in_world {
        return Ok(());
    }
    owner.gameobject_callback(object, GameObjectCallback::RemoveOutdoorAndOwner, context)?;
    remove_source_gameobject_model(owner, object, context)?;
    if owner.gameobjects().get(object)?.group.is_some() {
        // Group destruction/membership belongs to its live owner; GameObject.cpp:172.
        owner.go_clear_group(object, context)?;
        owner.gameobjects().get_mut(object)?.group = None;
    }
    Ok(())
}
fn reset_door<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    if matches!(owner.gameobjects().get(object)?.loot_state, 1 | 3) {
        return Ok(());
    }
    // ResetDoorOrButton toggles even when the current state is READY; GameObject.cpp:1337-1344,1385-1395.
    let value = owner.gameobjects().get_mut(object)?;
    value.flags &= !1;
    let next = if value.state == 1 { 0 } else { 1 };
    state(owner, object, next, context.grid)?;
    loot(owner, object, 3, None, context)?;
    owner.gameobjects().get_mut(object)?.cooldown_time = 0;
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateFlow {
    Tail,
    Return,
}
fn ready<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<UpdateFlow> {
    use GameObjectLifecyclePhase as Phase;
    let expiry = owner.gameobjects().get(object)?.respawn_time;
    if expiry > 0 && expiry <= wall(owner, object, GameObjectLifecycleClock::ReadyExpiry)? {
        owner.gameobjects().get_mut(object)?.respawn_time = 0;
        owner.gameobjects().get_mut(object)?.clear_uses();
        let kind = owner.gameobjects().get(object)?.template.kind;
        if kind == 17 {
            owner.go_lifecycle_phase(object, Phase::FishNotHooked, context)?;
            owner.gameobjects().get_mut(object)?.loot_state = 3;
            return Ok(UpdateFlow::Return);
        }
        if matches!(kind, 0 | 1) && owner.gameobjects().get(object)?.state != 1 {
            reset_door(owner, object, context)?;
        }
        if !owner.gameobjects().get(object)?.spawned_by_default {
            loot(owner, object, 3, None, context)?;
            if !context.grid.objects.get(object)?.database_spawn_data {
                owner.go_lifecycle_phase(object, Phase::RemoveWildOwner, context)?;
                delete_inner(owner, object, context)?;
            }
            return Ok(UpdateFlow::Return);
        }
        // Re-add the retained construction, then reinitialize AI; GameObject.cpp:424-426.
        gameobjects::add_source_gameobject(owner, object, context.grid)?;
        owner.gameobject_callback(object, GameObjectCallback::InitializeAi, context.grid)?;
    }
    if owner.gameobjects().get(object)?.is_spawned() {
        let value = owner.gameobjects().get(object)?;
        let (kind, state, cooldown, lock, diameter, trap_cooldown) = (
            value.template.kind,
            value.state,
            value.cooldown_time,
            value.template.data[0],
            value.template.data[2],
            value.template.data[5],
        );
        if kind == 6
            && state == 1
            && cooldown < wall(owner, object, GameObjectLifecycleClock::TrapCooldown)?
        {
            let mut radius = diameter as f32 / 2.0;
            if lock == 12 {
                radius = diameter as f32;
            }
            let mut valid = true;
            if radius == 0.0 {
                if trap_cooldown != 3 || owner.gameobjects().get(object)?.respawn_time > 0 {
                    valid = false;
                } else {
                    radius = trap_cooldown as f32;
                }
            }
            if valid {
                owner.go_lifecycle_phase(
                    object,
                    Phase::TrapSearch {
                        radius_bits: radius.to_bits(),
                    },
                    context,
                )?;
            }
        }
        let value = owner.gameobjects().get(object)?;
        let charges = value.template.charges();
        if charges > 0 && value.use_times >= charges as u32 {
            owner.gameobjects().get_mut(object)?.use_times = 0;
            loot(owner, object, 3, None, context)?;
        }
    }
    Ok(UpdateFlow::Tail)
}
fn deactivate<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<UpdateFlow> {
    use GameObjectLifecyclePhase as Phase;
    owner.go_lifecycle_phase(object, Phase::WorldStateRevert, context)?;
    if let Some(trap) = linked(owner, object, context.grid)? {
        loot(owner, trap, 3, None, context)?;
        delete_inner(owner, trap, context)?;
    }
    let kind = owner.gameobjects().get(object)?.template.kind;
    match kind {
        6 if owner.go_pending_events(object)? > 0 => return Ok(UpdateFlow::Tail),
        10 => {
            if owner.gameobjects().get(object)?.template.data[10] != 0 {
                owner.go_lifecycle_phase(object, Phase::GooberCasts, context)?;
                owner.gameobjects().get_mut(object)?.clear_uses();
            }
            state(owner, object, 1, context.grid)?;
        }
        29 if owner.gameobjects().get(object)?.template.data[0] != 0 => {
            owner.go_lifecycle_phase(object, Phase::CapturePointDeactivate, context)?;
            owner.gameobjects().get_mut(object)?.unique_users.clear();
            loot(owner, object, 1, None, context)?;
            return Ok(UpdateFlow::Return);
        }
        3 => {
            owner.gameobjects().get_mut(object)?.corpse_despawn_ms = i64::MAX;
            let value = owner.gameobjects().get(object)?;
            let (consumable, restock) = (value.template.data[3], value.template.data[2]);
            if consumable == 0 && restock != 0 {
                let now = wall(owner, object, GameObjectLifecycleClock::ChestRestock)?;
                owner.gameobjects().get_mut(object)?.restock_time =
                    seconds(now, i64::from(restock))?;
                loot(owner, object, 0, None, context)?;
                owner.go_lifecycle_phase(object, Phase::ForceDynamicUpdate, context)?;
                return Ok(UpdateFlow::Return);
            }
        }
        _ => {}
    }
    let value = owner.gameobjects().get(object)?;
    let action = value.template.despawn_at_action();
    if !context.grid.objects.get(object)?.database_spawn_data
        && (value.spell == 0 || value.template.despawn_possible() || action || value.forced_despawn)
    {
        owner.go_lifecycle_phase(object, Phase::RemoveWildOwner, context)?;
        delete_inner(owner, object, context)?;
        return Ok(UpdateFlow::Return);
    }
    if action || owner.gameobjects().get(object)?.animation > 0 {
        owner.go_lifecycle_phase(object, Phase::DespawnAnimation, context)?;
        let instance = context
            .persistent
            .state(context.spawns.persistent())?
            .entry
            .instanceable();
        let value = owner.gameobjects().get_mut(object)?;
        value.flags = if instance {
            (value.template.flags & !0x16) | (value.flags & 0x16)
        } else {
            value.template.flags
        };
    }
    owner.go_lifecycle_phase(object, Phase::DestroyLoot, context)?;
    owner.gameobjects().get_mut(object)?.has_loot = false;
    owner.go_lifecycle_phase(object, Phase::ClearLootRecipient, context)?;
    loot(owner, object, 1, None, context)?;
    let value = owner.gameobjects().get(object)?;
    if matches!(kind, 3 | 10) && !action && !value.forced_despawn {
        return Ok(UpdateFlow::Return);
    }
    if value.respawn_delay == 0 {
        return Ok(UpdateFlow::Return);
    }
    owner.go_lifecycle_phase(object, Phase::AiJustDespawned, context)?;
    owner.go_lifecycle_phase(object, Phase::InstanceDespawn, context)?;
    let value = owner.gameobjects().get(object)?;
    if !value.respawn_overridden {
        if value.spawned_by_default {
            let group_delay = if value.group.is_some() {
                owner.go_group_delay(object, context)?
            } else {
                None
            };
            if let Some(delay) = group_delay {
                owner.gameobjects().get_mut(object)?.respawn_delay = delay;
            } else if let Some(spawn) = owner.gameobjects().get(object)?.spawn.as_ref() {
                let (min, max) = (spawn.respawn_min as u32, spawn.respawn_max as u32);
                let delay = owner.gameobject_urand(min, max)?;
                owner.gameobjects().get_mut(object)?.respawn_delay = delay;
            }
        }
    } else if value.respawn_override_once {
        owner.gameobjects().get_mut(object)?.respawn_overridden = false;
    }
    let value = owner.gameobjects().get(object)?;
    let (by_default, delay, group, forced) = (
        value.spawned_by_default,
        value.respawn_delay,
        value.group.is_some(),
        value.forced_despawn,
    );
    let deadline = if by_default {
        seconds(
            wall(owner, object, GameObjectLifecycleClock::Deactivate)?,
            i64::from(delay),
        )?
    } else {
        0
    };
    owner.gameobjects().get_mut(object)?.respawn_time = deadline;
    if owner.go_save_immediately()? {
        save(owner, object, context)?;
    }
    let database = context.grid.objects.get(object)?.database_guid;
    if new_system(object, context.grid.objects)? {
        owner.gameobjects().get_mut(object)?.respawn_time = i64::MAX;
        if delay != 0 && !group {
            context.spawns.add(
                context.persistent,
                SpawnKey {
                    kind: GridSpawnKind::GameObject,
                    guid: database,
                },
                owner,
            )?;
        }
        if delay != 0 || !by_default || forced {
            context.grid.add_to_remove(object, owner)?;
        }
    } else if let Some(pool) = owner.go_pool(database)? {
        owner.go_lifecycle_phase(object, Phase::PoolRespawn(pool), context)?;
    }
    if context.grid.objects.get(object)?.in_world {
        owner.go_lifecycle_phase(object, Phase::Visibility, context)?;
    }
    owner.gameobjects().get_mut(object)?.forced_despawn = false;
    Ok(UpdateFlow::Tail)
}
fn update_inner<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>(
    owner: &mut F,
    object: ObjectHandle,
    diff: u32,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    use GameObjectLifecyclePhase as Phase;
    if context
        .grid
        .objects
        .get(object)?
        .guid
        .is_some_and(|guid| guid.high() == HighGuid::MoTransport as u16)
    {
        return Ok(());
    }
    owner.go_lifecycle_phase(object, Phase::Events(diff), context)?;
    let initial = owner.gameobjects().get(object)?.loot_state;
    let flow = match initial {
        0 => {
            let kind = owner.gameobjects().get(object)?.template.kind;
            match kind {
                6 => {
                    if owner.go_owner_in_combat(object)? {
                        let delay = owner.gameobjects().get(object)?.template.data[7];
                        let now = wall(owner, object, GameObjectLifecycleClock::TrapArm)?;
                        owner.gameobjects().get_mut(object)?.cooldown_time =
                            seconds(now, i64::from(delay))?;
                    }
                    owner.gameobjects().get_mut(object)?.loot_state = 1;
                }
                17 => {
                    let deadline = owner
                        .gameobjects()
                        .get(object)?
                        .respawn_time
                        .checked_sub(5)
                        .ok_or_else(|| {
                            refuse("execution.gameobject_lifecycle.signed_time_overflow_undefined")
                        })?;
                    if wall(owner, object, GameObjectLifecycleClock::FishingReady)? > deadline {
                        owner.go_lifecycle_phase(object, Phase::FishReady, context)?;
                        owner.gameobjects().get_mut(object)?.loot_state = 1;
                    }
                }
                3 => {
                    if owner.gameobjects().get(object)?.template.data[2] != 0 {
                        let restock = owner.gameobjects().get(object)?.restock_time;
                        if restock == 0 {
                            owner.gameobjects().get_mut(object)?.loot_state = 1;
                        } else if restock
                            <= wall(owner, object, GameObjectLifecycleClock::ChestRestock)?
                        {
                            let value = owner.gameobjects().get_mut(object)?;
                            value.restock_time = 0;
                            value.loot_state = 1;
                            owner.go_lifecycle_phase(object, Phase::DestroyLoot, context)?;
                            owner.gameobjects().get_mut(object)?.has_loot = false;
                            owner.go_lifecycle_phase(object, Phase::ForceDynamicUpdate, context)?;
                        }
                        return Ok(());
                    }
                    owner.gameobjects().get_mut(object)?.loot_state = 1;
                }
                _ => {}
            }
            // GO_NOT_READY falls through into GO_READY, even if its state did not change; GameObject.cpp:317-378.
            ready(owner, object, context)?
        }
        1 => ready(owner, object, context)?,
        2 => {
            let value = owner.gameobjects().get(object)?;
            let (kind, auto_close, cooldown, has_loot, despawn) = (
                value.template.kind,
                value.template.data[2],
                value.cooldown_time,
                value.has_loot,
                value.corpse_despawn_ms,
            );
            match kind {
                0 | 1
                    if auto_close / 65536 != 0
                        && cooldown < wall(owner, object, GameObjectLifecycleClock::AutoClose)? =>
                {
                    reset_door(owner, object, context)?
                }
                3 if has_loot => {
                    if despawn <= owner.world_clock_millis()? {
                        owner.gameobjects().get_mut(object)?.loot_state = 3;
                    }
                    owner.go_lifecycle_phase(object, Phase::LootUpdate, context)?;
                }
                6 => {
                    if owner.gameobjects().get(object)?.rearm_time == 0 {
                        let delay = owner.gameobjects().get(object)?.respawn_delay;
                        let deadline = seconds(
                            wall(owner, object, GameObjectLifecycleClock::TrapRearmStart)?,
                            i64::from(delay),
                        )?;
                        owner.gameobjects().get_mut(object)?.rearm_time = deadline;
                        state(owner, object, 2, context.grid)?;
                    }
                    if owner.gameobjects().get(object)?.rearm_time
                        < wall(owner, object, GameObjectLifecycleClock::TrapRearmCheck)?
                    {
                        state(owner, object, 1, context.grid)?;
                        let value = owner.gameobjects().get_mut(object)?;
                        value.loot_state = 1;
                        value.rearm_time = 0;
                    }
                }
                10 if cooldown < wall(owner, object, GameObjectLifecycleClock::GooberCooldown)? => {
                    owner.gameobjects().get_mut(object)?.flags &= !1;
                    loot(owner, object, 3, None, context)?;
                    owner.gameobjects().get_mut(object)?.cooldown_time = 0;
                }
                29 => {
                    let value = owner.gameobjects().get_mut(object)?;
                    value.capture_timer = value.capture_timer.wrapping_add(diff);
                    if value.capture_timer >= 5000 {
                        owner.go_lifecycle_phase(object, Phase::CaptureTick, context)?;
                        let timer = owner.gameobjects().get(object)?.capture_timer;
                        owner.gameobjects().get_mut(object)?.capture_timer =
                            timer.wrapping_sub(5000);
                    }
                }
                _ => {}
            }
            UpdateFlow::Tail
        }
        3 => deactivate(owner, object, context)?,
        _ => UpdateFlow::Tail,
    };
    if flow == UpdateFlow::Return {
        return Ok(());
    }
    let timer = owner.gameobjects().get(object)?.delayed_action_timer;
    if timer != 0 {
        if timer <= diff {
            owner.gameobjects().get_mut(object)?.delayed_action_timer = 0;
            owner.go_lifecycle_phase(object, Phase::DelayedAction, context)?;
        } else {
            owner.gameobjects().get_mut(object)?.delayed_action_timer -= diff;
        }
    }
    owner.go_lifecycle_phase(object, Phase::AiUpdate(diff), context)?;
    owner.go_lifecycle_phase(object, Phase::WorldObjectUpdate(diff), context)
}
pub fn update_source_gameobject<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    diff: u32,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| {
        if o.gameobjects().get(object)?.elevator.is_some() {
            update_source_elevator(o, object, c.grid)
        } else {
            update_inner(o, object, diff, c)
        }
    })
}
pub fn respawn_source_gameobject<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| {
        let value = o.gameobjects().get(object)?;
        if value.spawned_by_default && value.respawn_time > 0 {
            let now = wall(o, object, GameObjectLifecycleClock::Respawn)?;
            o.gameobjects().get_mut(object)?.respawn_time = now;
            c.persistent.save_respawn(
                c.spawns.persistent(),
                GridSpawnKind::GameObject,
                c.grid.objects.get(object)?.database_guid,
                0,
                o.world_game_seconds()?,
                o,
            )?;
        }
        Ok(())
    })
}
pub fn refresh_source_gameobject<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| {
        let value = o.gameobjects().get(object)?;
        if value.respawn_time > 0 && value.spawned_by_default {
            return Ok(());
        }
        if value.is_spawned() {
            gameobjects::add_source_gameobject(o, object, c.grid)?;
            o.gameobject_callback(object, GameObjectCallback::InitializeAi, c.grid)?;
        }
        Ok(())
    })
}
pub fn force_despawn_source_gameobject<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    delay_ms: u32,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, c| {
        if delay_ms != 0 {
            return o.go_lifecycle_phase(
                object,
                GameObjectLifecyclePhase::ScheduleForcedDespawn(delay_ms),
                c,
            );
        }
        o.gameobjects().get_mut(object)?.forced_despawn = true;
        loot(o, object, 3, None, c)?;
        let value = o.gameobjects().get(object)?;
        if value.respawn_delay == 0
            && c.grid.objects.get(object)?.database_guid != 0
            && !value.respawn_overridden
        {
            if let Some(spawn) = value.spawn.as_ref() {
                let (min, max) = (spawn.respawn_min as u32, spawn.respawn_max as u32);
                let delay = o.gameobject_urand(min, max)?;
                o.gameobjects()
                    .get_mut(object)?
                    .set_respawn_delay(delay, true);
            }
        }
        Ok(())
    })
}
/// The object dispatcher uses the same retained GO registry as construction.
pub struct GameObjectLifecycleDispatch<'a, D, P, F> {
    pub definitions: &'a mut D,
    pub products: &'a mut P,
    pub sets: &'a mut GridSpawnSets,
    pub persistent: &'a mut SourceManagers,
    pub spawns: &'a mut SourceSpawnManager,
    pub owner: &'a mut F,
}
impl<D: GridDefinitions, P: ResidencyProductOwner, F: GameObjectLifecycleOwner<D, P>>
    SourceObjectUpdateOwner for GameObjectLifecycleDispatch<'_, D, P, F>
{
    fn update(
        &mut self,
        object: ObjectHandle,
        diff: u32,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
    ) -> Result<()> {
        require(
            matches!(
                objects.get(object)?.kind,
                SourceObjectKind::GameObject | SourceObjectKind::Transport
            ),
            "execution.gameobject_lifecycle.other_virtual_update_owner",
        )?;
        update_source_gameobject(
            self.owner,
            object,
            diff,
            &mut GameObjectLifecycleContext {
                grid: &mut GridFactoryContext {
                    definitions: self.definitions,
                    products: self.products,
                    sets: self.sets,
                    objects,
                    cells,
                },
                persistent: self.persistent,
                spawns: self.spawns,
            },
        )
    }
}

pub fn set_source_gameobject_display<
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    display: u32,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    // Display state precedes model replacement; GameObject.cpp:2083-2086.
    owner.gameobjects().get_mut(object)?.display = display;
    update_source_gameobject_model(owner, object, grid)
}
pub fn destroy_source_gameobject_model<
    D: GridDefinitions,
    P: GridProductOwner,
    F: GameObjectFactoryOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    if owner.gameobjects().get(object)?.has_model {
        owner.gameobject_model(object, GameObjectModelOperation::Destroy, grid)?;
        owner.gameobjects().get_mut(object)?.has_model = false;
    }
    Ok(())
}

pub fn set_source_gameobject_respawn_time<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    delay: i64,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, _| {
        let deadline = if delay > 0 {
            seconds(
                wall(o, object, GameObjectLifecycleClock::SetRespawn)?,
                delay,
            )?
        } else {
            0
        };
        let value = o.gameobjects().get_mut(object)?;
        value.respawn_time = deadline;
        value.respawn_delay = if delay > 0 { delay as u32 } else { 0 };
        Ok(())
    })
}

pub fn set_source_gameobject_chest_despawn<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
) -> Result<()> {
    run(owner, object, context, |o, _| {
        // Chest loot lasts five snapshot-clock minutes; GameObject.cpp:754-756.
        let expiry = o
            .world_clock_millis()?
            .checked_add(300_000)
            .ok_or_else(|| refuse("execution.gameobject_lifecycle.timepoint_overflow_undefined"))?;
        o.gameobjects().get_mut(object)?.corpse_despawn_ms = expiry;
        Ok(())
    })
}
