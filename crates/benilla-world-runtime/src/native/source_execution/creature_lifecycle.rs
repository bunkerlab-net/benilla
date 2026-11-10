// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Retained Creature::Update/SetDeathState/RemoveCorpse, not durable continuation.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CreatureDeathState {
    Alive,
    Corpse,
    Dead,
}
#[derive(Clone, Copy, Debug)]
pub struct CreatureLifecycleSettings {
    pub save_immediately: bool,
    pub world_boss: bool,
    pub forever_corpse: bool,
    pub corpse_acceleration_ms: u32,
    pub pool: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CreatureLifecyclePending {
    PoolEffects,
    LinkingEffects,
    GroupEffects,
    UnitEffects,
    AiEffects,
    InstanceEffects,
    VisibilityEffects,
    SealedLifecycleEvidence,
}
impl CreatureLifecyclePending {
    pub fn refusal(self) -> Refusal {
        refuse(match self {
            Self::PoolEffects => "execution.creature_lifecycle.f4_09_pool_effects_pending",
            Self::LinkingEffects => "execution.creature_lifecycle.f4_10_linking_effects_pending",
            Self::GroupEffects => "execution.creature_lifecycle.f4_12_group_effects_pending",
            Self::UnitEffects => "execution.creature_lifecycle.f5_unit_effects_pending",
            Self::AiEffects => "execution.creature_lifecycle.f5_ai_effects_pending",
            Self::InstanceEffects => "execution.creature_lifecycle.f5_instance_effects_pending",
            Self::VisibilityEffects => "execution.creature_lifecycle.visibility_effects_pending",
            Self::SealedLifecycleEvidence => {
                "execution.creature_lifecycle.mr_mg_mpr_evidence_pending"
            }
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureLifecyclePhase {
    UnitJustDied,
    ClearTargetNpcFlagsAndUnmount,
    ClearGroup,
    CreatorCooldown,
    UnitCorpse,
    UnitDead,
    Visibility,
    DestroyLoot,
    LinkingDespawn,
    InstanceDespawn,
    InterruptMoving,
    VisibilityRemoveCorpse,
    VisibilityRestore,
    UnitUpdate(u32),
    LootUpdate,
    Regenerate(u32),
    ClearAggroLootAurasAndDynamicFlags,
    UnitAlive,
    ResetEntry,
    ResetKillerSpellHitsLootAndFaction,
    RestoreDamageSchoolFlagsAndAddon,
    MotionInitialize,
    AiJustRespawned,
    InstanceRespawn,
    LinkingRespawn,
    GroupRespawn,
    MapAdd,
    PoolRespawn(u32),
}
impl CreatureLifecyclePhase {
    pub fn missing_owner(self) -> Refusal {
        use CreatureLifecyclePending as P;
        match self {
            Self::ClearGroup | Self::GroupRespawn => P::GroupEffects,
            Self::LinkingDespawn | Self::LinkingRespawn => P::LinkingEffects,
            Self::PoolRespawn(_) => P::PoolEffects,
            Self::AiJustRespawned => P::AiEffects,
            Self::InstanceDespawn | Self::InstanceRespawn => P::InstanceEffects,
            Self::Visibility | Self::VisibilityRemoveCorpse | Self::VisibilityRestore => {
                P::VisibilityEffects
            }
            _ => P::UnitEffects,
        }
        .refusal()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureLifecycleClock {
    Death,
    Save,
    ScriptCorpseDelay,
    DeadUpdate,
    Respawn,
    SetRespawn,
}
/// The calling map's existing grid, persistent state and SpawnManager owners.
pub struct CreatureLifecycleContext<'a, 'b, D: GridDefinitions, P: ResidencyProductOwner> {
    pub grid: &'a mut GridFactoryContext<'b, D, P>,
    pub persistent: &'a mut SourceManagers,
    pub spawns: &'a mut SourceSpawnManager,
}
/// Hooks compute effects at their source calls; absent owners never succeed by default.
pub trait CreatureLifecycleOwner<D: GridDefinitions, P: ResidencyProductOwner>:
    ResidencyOwner<D, P> + SpawnWork
{
    fn lifecycle_random(&mut self) -> Result<&mut dyn CreatureRandom>;
    fn lifecycle_wall(
        &mut self,
        object: ObjectHandle,
        call: CreatureLifecycleClock,
    ) -> Result<ClockSample>;
    fn lifecycle_phase(
        &mut self,
        phase: CreatureLifecyclePhase,
        _creature: &mut SourceCreatureLifecycle,
        _context: &mut CreatureLifecycleContext<'_, '_, D, P>,
    ) -> Result<()> {
        Err(phase.missing_owner())
    }
    // Group delay selection includes its constant-without-max branch; SpawnGroup.cpp:474-484.
    fn lifecycle_group_delay(
        &mut self,
        _creature: &mut SourceCreatureLifecycle,
        _context: &mut CreatureLifecycleContext<'_, '_, D, P>,
    ) -> Result<Option<u32>> {
        Err(CreatureLifecyclePending::GroupEffects.refusal())
    }
    // AI writes the actual mutable uint32, not an observation; Creature.cpp:279-282.
    fn lifecycle_corpse_removed(
        &mut self,
        _creature: &mut SourceCreatureLifecycle,
        _delay: &mut u32,
        _context: &mut CreatureLifecycleContext<'_, '_, D, P>,
    ) -> Result<()> {
        Err(CreatureLifecyclePending::AiEffects.refusal())
    }
    fn lifecycle_can_spawn(
        &mut self,
        _creature: &mut SourceCreatureLifecycle,
        _context: &mut CreatureLifecycleContext<'_, '_, D, P>,
    ) -> Result<bool> {
        Err(CreatureLifecyclePending::LinkingEffects.refusal())
    }
}
#[derive(Debug)]
pub struct SourceCreatureLifecycle {
    pub construction: CreatureConstruction,
    pub settings: CreatureLifecycleSettings,
    pub death_state: CreatureDeathState,
    pub corpse_expiration_ms: i64,
    pub respawn_overridden: bool,
    pub respawn_override_once: bool,
    pub has_loot: bool,
    pub loot_status: u32,
    new_spawning_system: bool,
    stopped: bool,
}
impl SourceCreatureLifecycle {
    pub(super) fn new(
        construction: CreatureConstruction,
        settings: CreatureLifecycleSettings,
        objects: &SourceObjects,
    ) -> Result<Self> {
        let object = objects.get(construction.object)?;
        require(
            matches!(
                object.kind,
                SourceObjectKind::Creature | SourceObjectKind::Pet
            ) && object.database_guid == construction.spawn.database_guid,
            "execution.creature_lifecycle.construction_binding",
        )?;
        let runtime = object
            .guid
            .ok_or_else(|| refuse("execution.creature.guid_absent"))?
            .counter();
        let new_spawning_system = source_creature_new_spawning(object.database_guid, runtime);
        Ok(Self {
            construction,
            settings,
            death_state: if object.alive {
                CreatureDeathState::Alive
            } else {
                CreatureDeathState::Dead
            },
            corpse_expiration_ms: 0,
            respawn_overridden: false,
            respawn_override_once: false,
            has_loot: false,
            loot_status: 0,
            new_spawning_system,
            stopped: false,
        })
    }
    pub fn object(&self) -> ObjectHandle {
        self.construction.object
    }
    pub fn set_respawn_delay(&mut self, seconds: u32, once: bool) {
        // Preserve the one-shot flag after consumption; Creature.h:806, Creature.cpp:1875-1876.
        self.construction.respawn_delay = seconds;
        self.respawn_overridden = true;
        self.respawn_override_once = once;
    }
    // Zero clears without a clock read; Creature.h:801.
    pub fn set_respawn_time<D: GridDefinitions, P: ResidencyProductOwner>(
        &mut self,
        delay: u32,
        owner: &mut impl CreatureLifecycleOwner<D, P>,
    ) -> Result<()> {
        self.run(|this| {
            this.construction.respawn_time = if delay == 0 {
                0
            } else {
                Self::add_seconds(this.wall(CreatureLifecycleClock::SetRespawn, owner)?, delay)?
            };
            Ok(())
        })
    }
    pub fn respawn_save_state(&self) -> RespawnSaveState {
        RespawnSaveState::Creature {
            respawn_seconds: self.construction.respawn_time,
            new_spawning_system: self.new_spawning_system,
            corpse_expiry_ms: self.corpse_expiration_ms,
        }
    }
    fn run(&mut self, action: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        require(!self.stopped, "execution.creature_lifecycle.stopped")?;
        let result = action(self);
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    fn binding<D: GridDefinitions, P: ResidencyProductOwner>(
        &self,
        context: &CreatureLifecycleContext<'_, '_, D, P>,
    ) -> Result<()> {
        require(
            context.grid.objects.get(self.object())?.map == Some(context.grid.cells.map())
                && context.spawns.map() == context.grid.cells.map()
                && context
                    .persistent
                    .state(context.spawns.persistent())?
                    .map_binding()
                    == Some(context.grid.cells.map()),
            "execution.creature_lifecycle.map_binding",
        )
    }
    fn wall<D: GridDefinitions, P: ResidencyProductOwner>(
        &self,
        call: CreatureLifecycleClock,
        owner: &mut impl CreatureLifecycleOwner<D, P>,
    ) -> Result<i64> {
        let sample = owner.lifecycle_wall(self.object(), call)?;
        require(
            sample.kind() == ClockKind::WallSeconds,
            "execution.creature_lifecycle.wall_kind",
        )?;
        Ok(sample.value())
    }
    fn add_seconds(now: i64, seconds: u32) -> Result<i64> {
        now.checked_add(i64::from(seconds))
            .ok_or_else(|| refuse("execution.creature_lifecycle.signed_time_overflow_undefined"))
    }
    fn add_millis(now: i64, millis: i64) -> Result<i64> {
        now.checked_add(millis)
            .ok_or_else(|| refuse("execution.creature_lifecycle.timepoint_overflow_undefined"))
    }
    fn save_inner<D: GridDefinitions, P: ResidencyProductOwner, F: CreatureLifecycleOwner<D, P>>(
        &mut self,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        let object = context.grid.objects.get(self.object())?;
        if object.kind == SourceObjectKind::Pet || !object.database_spawn_data {
            return Ok(());
        }
        let deadline = if self.construction.respawn_time
            > self.wall(CreatureLifecycleClock::Save, owner)?
            || self.new_spawning_system
        {
            Some(self.construction.respawn_time)
        } else if owner.world_clock_millis()? < self.corpse_expiration_ms {
            // Convert milliseconds through system_clock nanoseconds; Creature.cpp:2293, chrono.h:1127-1132.
            Some(
                self.corpse_expiration_ms
                    .checked_mul(1_000_000)
                    .ok_or_else(|| {
                        refuse("execution.creature_lifecycle.timepoint_overflow_undefined")
                    })?
                    / 1_000_000_000,
            )
        } else {
            None
        };
        if let Some(deadline) = deadline {
            context.persistent.save_respawn(
                context.spawns.persistent(),
                GridSpawnKind::Creature,
                self.construction.spawn.database_guid,
                deadline,
                owner.world_game_seconds()?,
                owner,
            )?;
        }
        Ok(())
    }
    pub fn save_respawn<
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: CreatureLifecycleOwner<D, P>,
    >(
        &mut self,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(context)?;
            this.save_inner(context, owner)
        })
    }
    pub fn die<D: GridDefinitions, P: ResidencyProductOwner, F: CreatureLifecycleOwner<D, P>>(
        &mut self,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(context)?;
            require(
                this.death_state == CreatureDeathState::Alive,
                "execution.creature_lifecycle.death_state",
            )?;
            if !this.respawn_overridden {
                let group_delay = if this.construction.group.is_some() {
                    owner.lifecycle_group_delay(this, context)?
                } else {
                    None
                };
                this.construction.respawn_delay = if let Some(delay) = group_delay {
                    delay
                } else if context.grid.objects.get(this.object())?.database_spawn_data {
                    let [min, max] = this.construction.spawn.respawn_seconds;
                    owner.lifecycle_random()?.creature_urand(min, max)?
                } else {
                    this.construction.respawn_delay
                };
            } else if this.respawn_override_once {
                this.respawn_overridden = false;
            }
            let seconds = if this.settings.forever_corpse {
                7 * 24 * 60 * 60
            } else {
                this.construction.corpse_delay
            };
            this.corpse_expiration_ms =
                Self::add_millis(owner.world_clock_millis()?, i64::from(seconds) * 1000)?;
            this.construction.respawn_time = Self::add_seconds(
                this.wall(CreatureLifecycleClock::Death, owner)?,
                this.construction.respawn_delay,
            )?;
            // Save precedes the local sentinel and nongroup AddCreature; Creature.cpp:1882-1892.
            if this.settings.save_immediately || this.settings.world_boss {
                this.save_inner(context, owner)?;
            }
            if this.new_spawning_system {
                this.construction.respawn_time = i64::MAX;
                if this.construction.respawn_delay != 0 && this.construction.group.is_none() {
                    context.spawns.add(
                        context.persistent,
                        SpawnKey {
                            kind: GridSpawnKind::Creature,
                            guid: this.construction.spawn.database_guid,
                        },
                        owner,
                    )?;
                }
            }
            owner.lifecycle_phase(CreatureLifecyclePhase::UnitJustDied, this, context)?;
            context.grid.objects.mark_dead(this.object())?;
            owner.lifecycle_phase(
                CreatureLifecyclePhase::ClearTargetNpcFlagsAndUnmount,
                this,
                context,
            )?;
            if this.construction.group.is_some() {
                owner.lifecycle_phase(CreatureLifecyclePhase::ClearGroup, this, context)?;
                this.construction.group = None;
            }
            owner.lifecycle_phase(CreatureLifecyclePhase::CreatorCooldown, this, context)?;
            owner.lifecycle_phase(CreatureLifecyclePhase::UnitCorpse, this, context)?;
            this.death_state = CreatureDeathState::Corpse;
            Ok(())
        })
    }
    fn remove_inner<
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: CreatureLifecycleOwner<D, P>,
    >(
        &mut self,
        in_place: bool,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        if (!in_place && !context.grid.objects.get(self.object())?.in_world)
            || self.death_state != CreatureDeathState::Corpse
        {
            return Ok(());
        }
        self.corpse_expiration_ms = 0;
        owner.lifecycle_phase(CreatureLifecyclePhase::UnitDead, self, context)?;
        self.death_state = CreatureDeathState::Dead;
        owner.lifecycle_phase(CreatureLifecyclePhase::Visibility, self, context)?;
        owner.lifecycle_phase(CreatureLifecyclePhase::DestroyLoot, self, context)?;
        self.has_loot = false;
        self.loot_status = 0;
        let mut delay = 0;
        owner.lifecycle_corpse_removed(self, &mut delay, context)?;
        if self.construction.linking_trigger {
            owner.lifecycle_phase(CreatureLifecyclePhase::LinkingDespawn, self, context)?;
        }
        owner.lifecycle_phase(CreatureLifecyclePhase::InstanceDespawn, self, context)?;
        if delay != 0 {
            self.construction.respawn_time = Self::add_seconds(
                self.wall(CreatureLifecycleClock::ScriptCorpseDelay, owner)?,
                delay,
            )?;
            if self.settings.save_immediately || self.settings.world_boss {
                self.save_inner(context, owner)?;
            }
        }
        owner.lifecycle_phase(CreatureLifecyclePhase::InterruptMoving, self, context)?;
        let respawn = context
            .grid
            .objects
            .get(self.object())?
            .respawn_pose
            .ok_or_else(|| refuse("execution.relocation.respawn_definition_owner"))?;
        // CreatureRelocation, not a second cell mover or unload routine; Creature.cpp:304.
        let _ = context.grid.move_creature(self.object(), respawn, owner)?;
        self.construction.position = context
            .grid
            .objects
            .pose(self.object())?
            .0
            .map(f32::from_bits);
        for phase in [
            CreatureLifecyclePhase::VisibilityRemoveCorpse,
            CreatureLifecyclePhase::VisibilityRestore,
        ] {
            owner.lifecycle_phase(phase, self, context)?;
        }
        if self.new_spawning_system {
            context.grid.add_to_remove(self.object(), owner)?;
        }
        Ok(())
    }
    pub fn remove_corpse<
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: CreatureLifecycleOwner<D, P>,
    >(
        &mut self,
        in_place: bool,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(context)?;
            this.remove_inner(in_place, context, owner)
        })
    }
    pub fn update<D: GridDefinitions, P: ResidencyProductOwner, F: CreatureLifecycleOwner<D, P>>(
        &mut self,
        diff: u32,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(context)?;
            // Match once: corpse removal never runs DEAD respawn in this update; Creature.cpp:754-764.
            match this.death_state {
                CreatureDeathState::Alive => {
                    owner.lifecycle_phase(
                        CreatureLifecyclePhase::UnitUpdate(diff),
                        this,
                        context,
                    )?;
                    if this.death_state == CreatureDeathState::Alive {
                        owner.lifecycle_phase(
                            CreatureLifecyclePhase::Regenerate(diff),
                            this,
                            context,
                        )?;
                    }
                }
                CreatureDeathState::Corpse => {
                    owner.lifecycle_phase(
                        CreatureLifecyclePhase::UnitUpdate(diff),
                        this,
                        context,
                    )?;
                    if this.has_loot {
                        owner.lifecycle_phase(CreatureLifecyclePhase::LootUpdate, this, context)?;
                    }
                    if owner.world_clock_millis()? >= this.corpse_expiration_ms {
                        this.remove_inner(false, context, owner)?;
                    }
                }
                CreatureDeathState::Dead => {
                    if this.construction.respawn_time
                        > this.wall(CreatureLifecycleClock::DeadUpdate, owner)?
                    {
                        return Ok(());
                    }
                    if this.construction.spawning_linked
                        && !owner.lifecycle_can_spawn(this, context)?
                    {
                        return Ok(());
                    }
                    this.construction.respawn_time = 0;
                    owner.lifecycle_phase(
                        CreatureLifecyclePhase::ClearAggroLootAurasAndDynamicFlags,
                        this,
                        context,
                    )?;
                    this.has_loot = false;
                    owner.lifecycle_phase(CreatureLifecyclePhase::UnitAlive, this, context)?;
                    context.grid.objects.legacy_respawn(this.object())?;
                    this.death_state = CreatureDeathState::Alive;
                    for phase in [
                        CreatureLifecyclePhase::ResetEntry,
                        CreatureLifecyclePhase::ResetKillerSpellHitsLootAndFaction,
                        CreatureLifecyclePhase::RestoreDamageSchoolFlagsAndAddon,
                        CreatureLifecyclePhase::MotionInitialize,
                        CreatureLifecyclePhase::AiJustRespawned,
                        CreatureLifecyclePhase::InstanceRespawn,
                    ] {
                        owner.lifecycle_phase(phase, this, context)?;
                    }
                    if this.construction.linking_trigger {
                        owner.lifecycle_phase(
                            CreatureLifecyclePhase::LinkingRespawn,
                            this,
                            context,
                        )?;
                    }
                    if this.construction.group.is_some() {
                        owner.lifecycle_phase(
                            CreatureLifecyclePhase::GroupRespawn,
                            this,
                            context,
                        )?;
                    }
                    owner.lifecycle_phase(CreatureLifecyclePhase::MapAdd, this, context)?;
                    if context.grid.objects.get(this.object())?.kind != SourceObjectKind::Pet {
                        if let Some(pool) = this.settings.pool {
                            owner.lifecycle_phase(
                                CreatureLifecyclePhase::PoolRespawn(pool),
                                this,
                                context,
                            )?;
                        }
                    }
                }
            }
            Ok(())
        })
    }
    pub fn respawn<
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: CreatureLifecycleOwner<D, P>,
    >(
        &mut self,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(context)?;
            this.remove_inner(false, context, owner)?;
            let object = context.grid.objects.get(this.object())?;
            if object.in_world && this.death_state == CreatureDeathState::Dead {
                if object.database_spawn_data {
                    context.persistent.save_respawn(
                        context.spawns.persistent(),
                        GridSpawnKind::Creature,
                        object.database_guid,
                        0,
                        owner.world_game_seconds()?,
                        owner,
                    )?;
                }
                this.construction.respawn_time =
                    this.wall(CreatureLifecycleClock::Respawn, owner)?;
            }
            Ok(())
        })
    }
    pub fn inspecting_loot(&mut self, world_clock_ms: i64) -> Result<()> {
        self.run(|this| {
            if !this.settings.forever_corpse {
                let until = Self::add_millis(
                    world_clock_ms,
                    i64::from(this.settings.corpse_acceleration_ms),
                )?;
                this.corpse_expiration_ms = this.corpse_expiration_ms.max(until);
            }
            Ok(())
        })
    }
    pub fn reduce_corpse_decay(
        &mut self,
        world_clock_ms: i64,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.run(|this| {
            if objects.get(this.object())?.in_world && !this.settings.forever_corpse {
                let until = Self::add_millis(
                    world_clock_ms,
                    i64::from(this.settings.corpse_acceleration_ms),
                )?;
                this.corpse_expiration_ms = this.corpse_expiration_ms.min(until);
            }
            Ok(())
        })
    }
}
// Dynamic spawning is a GUID comparison, not an event/config flag; Object.cpp:3076-3078.
pub fn source_creature_new_spawning(database: u32, runtime: u32) -> bool {
    database != 0 && database != runtime
}

#[derive(Default)]
pub struct SourceCreatures {
    creatures: std::collections::BTreeMap<ObjectHandle, SourceCreatureLifecycle>,
}
impl SourceCreatures {
    pub fn retain(
        &mut self,
        construction: CreatureConstruction,
        settings: CreatureLifecycleSettings,
        objects: &SourceObjects,
    ) -> Result<()> {
        let handle = construction.object;
        require(
            !self.creatures.contains_key(&handle),
            "execution.creature_lifecycle.already_retained",
        )?;
        self.creatures
            .insert(handle, construction.into_lifecycle(settings, objects)?);
        Ok(())
    }
    pub fn get(&self, handle: ObjectHandle) -> Result<&SourceCreatureLifecycle> {
        self.creatures
            .get(&handle)
            .ok_or_else(|| refuse("execution.stale_creature_lifecycle"))
    }
    pub fn get_mut(&mut self, handle: ObjectHandle) -> Result<&mut SourceCreatureLifecycle> {
        self.creatures
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.stale_creature_lifecycle"))
    }
    // Retain fields until F3.11 destroys the authoritative construction; Creature.cpp:155-163.
    pub fn forget_destroyed(
        &mut self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<()> {
        require(
            objects.get(handle).is_err(),
            "execution.creature_lifecycle.still_constructed",
        )?;
        self.creatures
            .remove(&handle)
            .ok_or_else(|| refuse("execution.stale_creature_lifecycle"))?;
        Ok(())
    }
}
/// Creature-only object dispatch. Composite map dispatch supplies other virtual Update owners.
pub struct CreatureLifecycleDispatch<'a, D, P, F> {
    pub creatures: &'a mut SourceCreatures,
    pub definitions: &'a mut D,
    pub products: &'a mut P,
    pub sets: &'a mut GridSpawnSets,
    pub persistent: &'a mut SourceManagers,
    pub spawns: &'a mut SourceSpawnManager,
    pub owner: &'a mut F,
}
impl<D: GridDefinitions, P: ResidencyProductOwner, F: CreatureLifecycleOwner<D, P>>
    SourceObjectUpdateOwner for CreatureLifecycleDispatch<'_, D, P, F>
{
    fn update(
        &mut self,
        object: ObjectHandle,
        diff: u32,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
    ) -> Result<()> {
        require(
            objects.get(object)?.kind == SourceObjectKind::Creature,
            "execution.creature_lifecycle.other_virtual_update_owner",
        )?;
        let mut grid = GridFactoryContext {
            definitions: self.definitions,
            products: self.products,
            sets: self.sets,
            objects,
            cells,
        };
        self.creatures.get_mut(object)?.update(
            diff,
            &mut CreatureLifecycleContext {
                grid: &mut grid,
                persistent: self.persistent,
                spawns: self.spawns,
            },
            self.owner,
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Seal8CreatureLifecycleReport {
    pub adapter: &'static str,
    pub direct_lifecycle_sites: usize,
    pub independently_matched_transitions: u64,
    pub first_refusal: Refusal,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
pub fn inspect_seal8_unsealed_creature_lifecycle(
    root: &Path,
) -> Result<Seal8CreatureLifecycleReport> {
    let (held, raw) = metadata(&root.join("source-sites.json"), 1_048_576)?;
    require(
        Sha256Id::of(&raw).as_str()
            == "097b87217694b01c5a42c114358a7f30b2faf2062095640c0e50a4028e739cfa",
        "execution.creature_lifecycle.seal8_site_identity",
    )?;
    let sites: serde_json::Value = serde_json::from_slice(&raw)
        .map_err(|_| refuse("execution.creature_lifecycle.seal8_sites"))?;
    let direct = sites
        .as_object()
        .ok_or_else(|| refuse("execution.creature_lifecycle.seal8_sites"))?
        .values()
        .filter(|site| {
            let function = site["callsite"]["function"].as_str().unwrap_or("");
            [
                "Creature::SetDeathState(",
                "Creature::RemoveCorpse(",
                "Creature::Update(",
                "Creature::Respawn(",
                "Creature::SaveRespawnTime(",
                "Creature::InspectingLoot(",
                "Creature::ReduceCorpseDecayTimer(",
            ]
            .iter()
            .any(|name| function.contains(name))
        })
        .count();
    held.check()?;
    Ok(Seal8CreatureLifecycleReport {
        adapter: "benilla-seal8-08-unsealed-creature-lifecycle-coverage-v1",
        direct_lifecycle_sites: direct,
        independently_matched_transitions: 0,
        first_refusal: CreatureLifecyclePending::SealedLifecycleEvidence.refusal(),
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
