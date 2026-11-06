// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Source SpawnManager state and synchronous dispatch, without production admission.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SpawnKey {
    pub kind: GridSpawnKind,
    pub guid: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpawnInfo {
    pub key: SpawnKey,
    pub deadline_millis: i64,
    pub used: bool,
    pub in_use: bool,
}
impl SpawnInfo {
    fn is_used(self) -> bool {
        self.used || self.in_use
    }
}
#[derive(Clone, Copy, Debug)]
pub struct DynamicSpawnDefinition {
    pub map: u32,
    pub cell: CellPair,
    pub spawn_mask: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpawnGroupHandle {
    pub id: u32,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct DynamicSpawnGroup {
    pub id: u32,
    pub kind: GridSpawnKind,
    pub first_guid: Option<u32>,
    pub string_id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicSpawnRequest {
    pub map: MapIdentity,
    pub thread: u64,
    pub key: SpawnKey,
}
impl DynamicSpawnRequest {
    // Object.cpp:2183,2207 explicitly pass zero, independently of event membership.
    pub fn requested_guid(self) -> u32 {
        0
    }
    pub fn spawn_creature<D, P, F>(
        self,
        factory: &mut F,
        attempt: GridLoadAttempt,
        row: D::Row,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<bool>
    where
        D: GridDefinitions,
        P: GridProductOwner,
        F: CreatureFactoryOwner<D, P>,
    {
        require(
            self.map == context.cells.map()
                && self.map == attempt.grid.map
                && self.thread == factory.creature_thread()
                && self.key
                    == SpawnKey {
                        kind: attempt.kind,
                        guid: attempt.guid,
                    },
            "execution.spawn.constructor_binding",
        )?;
        Ok(matches!(
            spawn_source_creature(factory, attempt, row, context)?,
            Some(GridCandidateOutcome::Loaded(_))
        ))
    }
    pub fn spawn_gameobject<D, P, F>(
        self,
        factory: &mut F,
        attempt: GridLoadAttempt,
        row: D::Row,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<bool>
    where
        D: GridDefinitions,
        P: GridProductOwner,
        F: GameObjectFactoryOwner<D, P>,
    {
        require(
            self.map == context.cells.map()
                && self.map == attempt.grid.map
                && self.thread == factory.gameobject_thread()
                && self.key
                    == SpawnKey {
                        kind: attempt.kind,
                        guid: attempt.guid,
                    },
            "execution.spawn.constructor_binding",
        )?;
        Ok(matches!(
            spawn_source_gameobject(factory, attempt, row, context)?,
            Some(GridCandidateOutcome::Loaded(_))
        ))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnClockCall {
    Initialize,
    Respawn(SpawnKey),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SpawnPending {
    EffectiveDefinitions,
    LinkingOwner,
    GroupOwner,
    ConstructorComposition,
    ClockLineage,
    SealedExecution,
    DurableContinuation,
    ActiveVectorLifetime,
}
impl SpawnPending {
    pub fn id(self) -> &'static str {
        match self {
            Self::EffectiveDefinitions => "execution.spawn.effective_definitions_owner",
            Self::LinkingOwner => "execution.spawn.linking_owner",
            Self::GroupOwner => "execution.spawn.group_owner",
            Self::ConstructorComposition => "execution.spawn.constructor_composition_owner",
            Self::ClockLineage => "execution.spawn.clock_lineage_owner",
            Self::SealedExecution => "execution.f1_binding_pending",
            Self::DurableContinuation => "execution.managers.f7_continuation_owner",
            Self::ActiveVectorLifetime => "execution.spawn.active_vector_lifetime_unbound",
        }
    }
    pub fn refusal(self) -> Refusal {
        refuse(self.id())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SpawnOperation {
    Add {
        deferred: bool,
        index: usize,
        info: SpawnInfo,
    },
    Used {
        index: usize,
        info: SpawnInfo,
    },
    Deadline {
        index: usize,
        before: i64,
        after: i64,
    },
    Construct {
        index: usize,
        info: SpawnInfo,
        success: Option<bool>,
    },
    Erase {
        index: usize,
        info: SpawnInfo,
    },
    EventGuid {
        key: SpawnKey,
        insert: bool,
        changed: bool,
    },
    DeferredAppend {
        count: usize,
    },
    Update {
        begin: bool,
    },
    Initialized,
    Group {
        handle: SpawnGroupHandle,
        create: bool,
    },
    GroupVicinity {
        handle: SpawnGroupHandle,
        position_bits: [u32; 4],
        range_bits: u32,
    },
}
/// Every dependency executes its source operation. No captured gameplay answer supplies an owner.
pub trait SpawnWork: PersistentEffects {
    fn thread(&self) -> u64;
    fn wall_seconds(&mut self, call: SpawnClockCall) -> Result<ClockSample>;
    fn world_clock_millis(&mut self) -> Result<i64>;
    fn world_game_seconds(&mut self) -> Result<i64>;
    fn dynamic_guids(&mut self, map: u32, kind: GridSpawnKind) -> Result<Arc<[u32]>>;
    fn definition(&mut self, key: SpawnKey) -> Result<Option<DynamicSpawnDefinition>>;
    fn linking_can_spawn(&mut self, guid: u32) -> Result<bool>;
    fn groups(&mut self) -> Result<Arc<BTreeMap<u32, DynamicSpawnGroup>>>;
    fn create_group(&mut self, group: DynamicSpawnGroup) -> Result<SpawnGroupHandle>;
    fn update_group(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        group: SpawnGroupHandle,
    ) -> Result<()>;
    fn respawn_group_in_vicinity(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        group: SpawnGroupHandle,
        position_bits: [u32; 4],
        range_bits: u32,
    ) -> Result<()>;
    fn construct(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        request: DynamicSpawnRequest,
    ) -> Result<bool>;
    fn spawn_operation(&mut self, map: MapIdentity, operation: SpawnOperation) -> Result<()>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpawnInventory {
    pub version: u32,
    pub map: MapIdentity,
    pub persistent: PersistentStateIdentity,
    pub initialization_started: bool,
    pub initialized: bool,
    pub active: Vec<SpawnInfo>,
    pub deferred: Vec<SpawnInfo>,
    pub updating: bool,
    pub stopped: bool,
    pub constructing_depth: usize,
    pub held_max_index: Option<usize>,
    pub bound_active_capacity: Option<usize>,
    pub groups: BTreeMap<u32, SpawnGroupHandle>,
    pub string_groups: BTreeMap<u32, SpawnGroupHandle>,
    pub creature_events: BTreeSet<u32>,
    pub gameobject_events: BTreeSet<u32>,
}
impl SpawnInventory {
    pub fn validate(&self) -> Result<()> {
        require(self.version == 1, "execution.spawn.inventory_version")?;
        require(
            self.map.creation_generation > 0
                && self.persistent.generation > 0
                && self.map.map == self.persistent.map
                && self.map.instance == self.persistent.instance,
            "execution.spawn.inventory_identity",
        )?;
        require(
            !self.initialized || self.initialization_started,
            "execution.spawn.inventory_initialization",
        )?;
        require(
            self.bound_active_capacity
                .is_none_or(|capacity| capacity >= self.active.len()),
            "execution.spawn.inventory_capacity",
        )?;
        require(
            self.deferred.iter().all(|info| !info.in_use),
            "execution.spawn.inventory_in_use",
        )?;
        require(
            (self.constructing_depth == 0) == self.held_max_index.is_none()
                && self
                    .held_max_index
                    .is_none_or(|index| index < self.active.len()),
            "execution.spawn.inventory_construction",
        )?;
        require(
            self.groups
                .values()
                .chain(self.string_groups.values())
                .all(|group| group.generation > 0),
            "execution.spawn.inventory_group",
        )
    }
    pub fn require_quiescent(&self) -> Result<()> {
        self.validate()?;
        require(
            !self.stopped
                && !self.updating
                && self.constructing_depth == 0
                && !self.active.iter().any(|info| info.in_use),
            "execution.spawn.nonquiescent",
        )?;
        Err(SpawnPending::DurableContinuation.refusal())
    }
}
pub struct SourceSpawnManager {
    state: SpawnInventory,
}
impl SourceSpawnManager {
    pub fn new(map: MapIdentity, persistent: PersistentStateIdentity) -> Result<Self> {
        let state = SpawnInventory {
            version: 1,
            map,
            persistent,
            initialization_started: false,
            initialized: false,
            active: Vec::new(),
            deferred: Vec::new(),
            updating: false,
            stopped: false,
            constructing_depth: 0,
            held_max_index: None,
            bound_active_capacity: None,
            groups: BTreeMap::new(),
            string_groups: BTreeMap::new(),
            creature_events: BTreeSet::new(),
            gameobject_events: BTreeSet::new(),
        };
        state.validate()?;
        Ok(Self { state })
    }
    pub fn map(&self) -> MapIdentity {
        self.state.map
    }
    pub fn persistent(&self) -> PersistentStateIdentity {
        self.state.persistent
    }
    pub fn inventory(&self) -> SpawnInventory {
        self.state.clone()
    }
    fn run<T>(&mut self, action: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        require(!self.state.stopped, "execution.spawn.stopped")?;
        let result = action(self);
        self.state.stopped |= result.is_err();
        result
    }
    fn binding(&self, persistent: &SourceManagers, work: &impl SpawnWork) -> Result<()> {
        require(
            work.thread() > 0
                && persistent.state(self.persistent())?.map_binding() == Some(self.map()),
            "execution.spawn.map_binding",
        )
    }
    fn observe(&self, work: &mut impl SpawnWork, operation: SpawnOperation) -> Result<()> {
        work.spawn_operation(self.map(), operation)
    }
    fn wall(work: &mut impl SpawnWork, call: SpawnClockCall) -> Result<i64> {
        let sample = work.wall_seconds(call)?;
        require(
            sample.kind() == ClockKind::WallSeconds,
            "execution.spawn.wall_clock_owner",
        )?;
        Ok(sample.value())
    }
    /// Independent storage-history evidence, not Rust Vec capacity or a gameplay result.
    pub fn bind_active_capacity(&mut self, capacity: usize) -> Result<()> {
        self.run(|this| {
            require(
                !this.state.updating
                    && this.state.constructing_depth == 0
                    && capacity >= this.state.active.len(),
                "execution.spawn.capacity_binding",
            )?;
            this.state.bound_active_capacity = Some(capacity);
            Ok(())
        })
    }
    fn add_inner(
        &mut self,
        persistent: &SourceManagers,
        key: SpawnKey,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.binding(persistent, work)?;
        let deadline_millis = persistent
            .state(self.persistent())?
            .respawn(key.kind, key.guid)
            .checked_mul(1000)
            .ok_or_else(|| refuse("execution.spawn.timepoint_overflow"))?;
        let info = SpawnInfo {
            key,
            deadline_millis,
            used: false,
            in_use: false,
        };
        let deferred = self.state.updating;
        if !deferred {
            // Immediate ConstructForMap retains a vector reference; .cpp:112-127,152-153,178-179,30-40.
            if self.state.constructing_depth != 0 {
                require(
                    self.state
                        .bound_active_capacity
                        .is_some_and(|capacity| self.state.active.len() < capacity),
                    SpawnPending::ActiveVectorLifetime.id(),
                )?;
            }
            if self
                .state
                .bound_active_capacity
                .is_some_and(|capacity| self.state.active.len() >= capacity)
            {
                self.state.bound_active_capacity = None;
            }
        }
        let vector = if deferred {
            &mut self.state.deferred
        } else {
            &mut self.state.active
        };
        let index = vector.len();
        vector.push(info);
        self.observe(
            work,
            SpawnOperation::Add {
                deferred,
                index,
                info,
            },
        )
    }
    pub fn add(
        &mut self,
        persistent: &SourceManagers,
        key: SpawnKey,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| this.add_inner(persistent, key, work))
    }
    pub fn initialize(
        &mut self,
        persistent: &mut SourceManagers,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(persistent, work)?;
            require(
                !this.state.initialization_started,
                "execution.spawn.initialization_repeated",
            )?;
            this.state.initialization_started = true;
            let now = Self::wall(work, SpawnClockCall::Initialize)?;
            // Creatures precede GOs. Initial expiry is strict; SpawnManager.cpp:50-82.
            for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
                for &guid in work.dynamic_guids(this.map().map, kind)?.iter() {
                    let key = SpawnKey { kind, guid };
                    if persistent.state(this.persistent())?.respawn(kind, guid) < now {
                        let data = work
                            .definition(key)?
                            .ok_or_else(|| refuse("execution.spawn.initial_definition_absent"))?;
                        let allowed = match kind {
                            GridSpawnKind::Creature => work.linking_can_spawn(guid)?,
                            GridSpawnKind::GameObject => data.spawn_mask != 0,
                        };
                        if allowed {
                            persistent.overlay(
                                this.persistent(),
                                data.cell,
                                kind,
                                guid,
                                true,
                                work,
                            )?;
                        }
                    } else {
                        this.add_inner(persistent, key, work)?;
                    }
                }
            }
            // Map-key traversal and first-member map filtering; .cpp:84-109.
            for &group in work.groups()?.values() {
                let Some(guid) = group.first_guid else {
                    continue;
                };
                let data = work
                    .definition(SpawnKey {
                        kind: group.kind,
                        guid,
                    })?
                    .ok_or_else(|| refuse("execution.spawn.group_definition_absent"))?;
                if data.map != this.map().map {
                    continue;
                }
                let handle = work.create_group(group)?;
                require(
                    handle.generation > 0 && handle.id == group.id,
                    "execution.spawn.group_binding",
                )?;
                this.state.groups.entry(group.id).or_insert(handle);
                if group.string_id != 0 {
                    this.state
                        .string_groups
                        .entry(group.string_id)
                        .or_insert(handle);
                }
                this.observe(
                    work,
                    SpawnOperation::Group {
                        handle,
                        create: true,
                    },
                )?;
            }
            this.state.initialized = true;
            this.observe(work, SpawnOperation::Initialized)
        })
    }
    fn construct(
        &mut self,
        index: usize,
        persistent: &mut SourceManagers,
        work: &mut impl SpawnWork,
    ) -> Result<bool> {
        // Track held source references without changing IsUsed or entry selection.
        let previous_depth = self.state.constructing_depth;
        let previous_max = self.state.held_max_index;
        self.state.constructing_depth += 1;
        self.state.held_max_index = Some(previous_max.map_or(index, |held| held.max(index)));
        self.state.active[index].in_use = true;
        let info = self.state.active[index];
        self.observe(
            work,
            SpawnOperation::Construct {
                index,
                info,
                success: None,
            },
        )?;
        let request = DynamicSpawnRequest {
            map: self.map(),
            thread: work.thread(),
            key: info.key,
        };
        let success = work.construct(self, persistent, request)?;
        if success {
            self.state.active[index].used = true;
        }
        self.state.active[index].in_use = false;
        self.state.constructing_depth = previous_depth;
        self.state.held_max_index = previous_max;
        self.observe(
            work,
            SpawnOperation::Construct {
                index,
                info: self.state.active[index],
                success: Some(success),
            },
        )?;
        Ok(success)
    }
    fn erase(&mut self, index: usize, refusal: &str, work: &mut impl SpawnWork) -> Result<()> {
        require(
            self.state.held_max_index.is_none_or(|held| index > held),
            refusal,
        )?;
        let info = self.state.active.remove(index);
        self.observe(work, SpawnOperation::Erase { index, info })
    }
    pub fn update(
        &mut self,
        persistent: &mut SourceManagers,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(persistent, work)?;
            require(this.state.initialized, "execution.spawn.not_initialized")?;
            this.state.updating = true;
            this.observe(work, SpawnOperation::Update { begin: true })?;
            let count = this.state.deferred.len();
            if count != 0 {
                if this.state.constructing_depth != 0 {
                    require(
                        this.state
                            .bound_active_capacity
                            .is_some_and(|capacity| capacity >= this.state.active.len() + count),
                        SpawnPending::ActiveVectorLifetime.id(),
                    )?;
                }
                if this
                    .state
                    .bound_active_capacity
                    .is_some_and(|capacity| this.state.active.len() + count > capacity)
                {
                    this.state.bound_active_capacity = None;
                }
                this.state.active.append(&mut this.state.deferred);
                this.observe(work, SpawnOperation::DeferredAppend { count })?;
            }
            let now = work.world_clock_millis()?;
            let mut index = 0;
            while index < this.state.active.len() {
                let info = this.state.active[index];
                // Do not sort or recheck a used flag written by a failed constructor; .cpp:270-277.
                if info.is_used()
                    || (info.deadline_millis <= now && this.construct(index, persistent, work)?)
                {
                    this.erase(index, "execution.spawn.recursive_update", work)?;
                } else {
                    index += 1;
                }
            }
            this.state.updating = false;
            this.observe(work, SpawnOperation::Update { begin: false })?;
            let mut after = None;
            loop {
                let next = this.group_after(after);
                let Some((id, handle)) = next else {
                    break;
                };
                after = Some(id);
                work.update_group(this, persistent, handle)?;
                this.observe(
                    work,
                    SpawnOperation::Group {
                        handle,
                        create: false,
                    },
                )?;
            }
            Ok(())
        })
    }
    pub fn respawn(
        &mut self,
        persistent: &mut SourceManagers,
        key: SpawnKey,
        delay: u32,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(persistent, work)?;
            let found = this
                .state
                .active
                .iter()
                .position(|info| !info.is_used() && info.key == key);
            let deadline = Self::wall(work, SpawnClockCall::Respawn(key))?
                .checked_add(i64::from(delay))
                .ok_or_else(|| refuse("execution.spawn.deadline_overflow"))?;
            // Save uses the World game-time snapshot, not the independent time() observation.
            let world_seconds = work.world_game_seconds()?;
            persistent.save_respawn(
                this.persistent(),
                key.kind,
                key.guid,
                deadline,
                world_seconds,
                work,
            )?;
            match found {
                None => this.add_inner(persistent, key, work),
                Some(index) if delay == 0 => this.construct(index, persistent, work).map(|_| ()),
                Some(index) => {
                    let after = work
                        .world_clock_millis()?
                        .checked_add(i64::from(delay) * 1000)
                        .ok_or_else(|| refuse("execution.spawn.timepoint_overflow"))?;
                    let before = this.state.active[index].deadline_millis;
                    this.state.active[index].deadline_millis = after;
                    this.observe(
                        work,
                        SpawnOperation::Deadline {
                            index,
                            before,
                            after,
                        },
                    )
                }
            }
        })
    }
    pub fn respawn_all(
        &mut self,
        persistent: &mut SourceManagers,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(persistent, work)?;
            let mut index = 0;
            while index < this.state.active.len() {
                let key = this.state.active[index].key;
                let world_seconds = work.world_game_seconds()?;
                persistent.save_respawn(
                    this.persistent(),
                    key.kind,
                    key.guid,
                    0,
                    world_seconds,
                    work,
                )?;
                if this.construct(index, persistent, work)? {
                    this.erase(
                        index,
                        "execution.spawn.recursive_respawn_all_lifetime",
                        work,
                    )?;
                } else {
                    index += 1;
                }
            }
            Ok(())
        })
    }
    pub fn remove_spawn(&mut self, key: SpawnKey, work: &mut impl SpawnWork) -> Result<()> {
        self.run(|this| {
            if let Some(index) = this.state.active.iter().position(|info| info.key == key) {
                this.state.active[index].used = true;
                this.observe(
                    work,
                    SpawnOperation::Used {
                        index,
                        info: this.state.active[index],
                    },
                )?;
            }
            Ok(())
        })
    }
    pub fn remove_spawns(
        &mut self,
        creatures: &[u32],
        gameobjects: &[u32],
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| {
            for index in 0..this.state.active.len() {
                let key = this.state.active[index].key;
                if match key.kind {
                    GridSpawnKind::Creature => creatures.contains(&key.guid),
                    GridSpawnKind::GameObject => gameobjects.contains(&key.guid),
                } {
                    this.state.active[index].used = true;
                    this.observe(
                        work,
                        SpawnOperation::Used {
                            index,
                            info: this.state.active[index],
                        },
                    )?;
                }
            }
            Ok(())
        })
    }
    pub fn event_guid(&self, guid: u32, high: HighGuid) -> bool {
        match high {
            HighGuid::Creature => self.state.creature_events.contains(&guid),
            HighGuid::GameObject => self.state.gameobject_events.contains(&guid),
            _ => false,
        }
    }
    pub fn set_event_guid(
        &mut self,
        guid: u32,
        high: HighGuid,
        insert: bool,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| {
            let (kind, set) = match high {
                HighGuid::Creature => (GridSpawnKind::Creature, &mut this.state.creature_events),
                HighGuid::GameObject => {
                    (GridSpawnKind::GameObject, &mut this.state.gameobject_events)
                }
                _ => return Ok(()),
            };
            let changed = if insert {
                set.insert(guid)
            } else {
                set.remove(&guid)
            };
            this.observe(
                work,
                SpawnOperation::EventGuid {
                    key: SpawnKey { kind, guid },
                    insert,
                    changed,
                },
            )
        })
    }
    pub fn group(&self, id: u32) -> Option<SpawnGroupHandle> {
        self.state.groups.get(&id).copied()
    }
    pub fn string_group(&self, id: u32) -> Option<SpawnGroupHandle> {
        self.state.string_groups.get(&id).copied()
    }
    fn group_after(&self, after: Option<u32>) -> Option<(u32, SpawnGroupHandle)> {
        if let Some(after) = after {
            self.state.groups.range((Excluded(after), Unbounded)).next()
        } else {
            self.state.groups.first_key_value()
        }
        .map(|(&id, &handle)| (id, handle))
    }
    // SpawnManager.cpp:332-336 dispatches every group, leaving range decisions to its owner.
    pub fn respawn_groups_in_vicinity(
        &mut self,
        persistent: &mut SourceManagers,
        position_bits: [u32; 4],
        range_bits: u32,
        work: &mut impl SpawnWork,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(persistent, work)?;
            let mut after = None;
            while let Some((id, handle)) = this.group_after(after) {
                after = Some(id);
                work.respawn_group_in_vicinity(
                    this,
                    persistent,
                    handle,
                    position_bits,
                    range_bits,
                )?;
                this.observe(
                    work,
                    SpawnOperation::GroupVicinity {
                        handle,
                        position_bits,
                        range_bits,
                    },
                )?;
            }
            Ok(())
        })
    }
}
