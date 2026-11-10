// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Effective spawn-group loading and retained map-local selection. No native admission.
use super::super::production_compiler::{SpawnGroupCatalogRows, VerifiedCatalog};
use super::*;
use benilla_sim::resident_map::CellPair;
use benilla_sim::source_spawn_groups::{
    self as selection, Formation, GroupInput, GroupInputData, GroupMember, GroupPhase, GroupRandom,
    GroupTemplate, RandomEntry, SelectionError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod group_links;
pub use group_links::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GroupSpawnDefinition {
    pub map: u32,
    pub cell: CellPair,
    pub position_bits: [u32; 3],
    pub entry: u32,
    pub spawn_mask: u8,
    pub game_event: i16,
    pub guid_pool: u16,
    pub entry_pool: u16,
}
impl GroupSpawnDefinition {
    fn excluded(self) -> bool {
        self.game_event != 0 || self.guid_pool != 0 || self.entry_pool != 0
    }
}
pub trait GroupRows {
    fn next(&mut self, phase: GroupPhase) -> Result<Option<GroupInput>>;
}
impl GroupRows for SpawnGroupCatalogRows {
    fn next(&mut self, phase: GroupPhase) -> Result<Option<GroupInput>> {
        Ok(SpawnGroupCatalogRows::next(self, phase)?)
    }
}
/// Real ObjectMgr/condition/script owners. Source metadata, not active-event answers.
pub trait GroupLoadOwner {
    fn condition_exists(&mut self, id: i32) -> Result<bool>;
    fn expression_exists(&mut self, id: i32) -> Result<bool>;
    fn string_exists(&mut self, id: u32) -> Result<bool>;
    fn spawn(&mut self, key: SpawnKey) -> Result<Option<GroupSpawnDefinition>>;
    fn template_exists(&mut self, kind: GridSpawnKind, entry: u32) -> Result<bool>;
    fn autoload_active(&mut self) -> Result<bool>;
    fn creature_template_flags(&mut self, entry: u32) -> Result<Option<u32>>;
    fn has_random_entries(&mut self, key: SpawnKey) -> Result<bool>;
    /// ObjectMgr.cpp:1500-1502,1528-1530: static-grid removal, then dynamic-vector erase.
    fn detach_spawn(&mut self, key: SpawnKey, definition: GroupSpawnDefinition) -> Result<()>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GroupSquad {
    pub id: u32,
    pub entries: BTreeMap<u32, u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpawnGroupDefinition {
    pub template: GroupTemplate,
    pub ordinal: u64,
    pub formation: Option<Formation>,
    pub members: Vec<GroupMember>,
    pub entries: Vec<RandomEntry>,
    pub equal: Vec<usize>,
    pub explicit: Vec<usize>,
    pub linked: Vec<u32>,
    pub squads: Vec<GroupSquad>,
    pub active: bool,
    pub enabled: bool,
    pub chanced: bool,
}
impl SpawnGroupDefinition {
    pub fn kind(&self) -> GridSpawnKind {
        if self.template.kind == 0 {
            GridSpawnKind::Creature
        } else {
            GridSpawnKind::GameObject
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GroupLoadDecision {
    Accepted,
    EmptyName,
    InvalidType,
    MissingCondition,
    MissingExpression,
    StringCleared,
    MissingGroup,
    FormationType,
    MovementType,
    FormationSpread,
    DuplicateMember,
    MissingSpawn,
    PoolOrEvent,
    MissingTemplate,
    MissingLinkedGroup,
    SlotCorrected,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GroupLoadRecord {
    pub phase: GroupPhase,
    pub ordinal: u64,
    pub group: u32,
    pub decision: GroupLoadDecision,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct GroupIdentity {
    pub map: MapIdentity,
    pub handle: SpawnGroupHandle,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpawnGroupState {
    pub identity: GroupIdentity,
    pub persistent: PersistentStateIdentity,
    pub objects: BTreeMap<u32, u32>,
    pub chosen_entries: BTreeMap<u32, u32>,
    pub chosen_spawns: BTreeMap<u32, bool>,
    pub chosen_squad: i32,
    pub enabled: bool,
    pub cooldown_millis: i64,
    pub constructing: bool,
    pub has_formation: bool,
    pub linkage_targets: BTreeSet<ObjectHandle>,
}
#[derive(Default)]
pub struct SourceSpawnGroups {
    started: bool,
    loaded: bool,
    stopped: bool,
    definitions: Arc<BTreeMap<u32, SpawnGroupDefinition>>,
    by_guid: BTreeMap<SpawnKey, u32>,
    dynamic: Arc<BTreeMap<u32, DynamicSpawnGroup>>,
    states: BTreeMap<(MapIdentity, u32), SpawnGroupState>,
    next_generation: u64,
    records: Vec<GroupLoadRecord>,
    frames: Vec<GroupIdentity>,
    held_members: Vec<(GroupIdentity, u32)>,
}
impl SourceSpawnGroups {
    fn run<T>(&mut self, action: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        require(!self.stopped, "execution.group.stopped")?;
        let result = action(self);
        self.stopped |= result.is_err();
        result
    }
    fn loaded(&self) -> Result<()> {
        require(self.loaded, "execution.group.loader_owner")
    }
    pub fn definitions(&self) -> Result<&Arc<BTreeMap<u32, SpawnGroupDefinition>>> {
        self.loaded()?;
        Ok(&self.definitions)
    }
    pub fn dynamic_groups(&self) -> Result<Arc<BTreeMap<u32, DynamicSpawnGroup>>> {
        self.loaded()?;
        Ok(self.dynamic.clone())
    }
    pub fn load_records(&self) -> &[GroupLoadRecord] {
        &self.records
    }
    fn record(&mut self, phase: GroupPhase, ordinal: u64, group: u32, decision: GroupLoadDecision) {
        self.records.push(GroupLoadRecord {
            phase,
            ordinal,
            group,
            decision,
        });
    }
    pub fn load_catalog(
        &mut self,
        catalog: &mut VerifiedCatalog,
        occurrences: &Path,
        owner: &mut impl GroupLoadOwner,
    ) -> Result<()> {
        self.load(&mut catalog.execution_spawn_group_rows(occurrences)?, owner)
    }
    pub fn load(
        &mut self,
        rows: &mut impl GroupRows,
        owner: &mut impl GroupLoadOwner,
    ) -> Result<()> {
        self.run(|this| {
            require(!this.started, "execution.group.loader_repeated")?;
            this.started = true;
            let mut definitions = BTreeMap::<u32, SpawnGroupDefinition>::new();
            let mut found = BTreeSet::new();
            let mut consumed = 0_u64;
            for phase in GroupPhase::ALL {
                while let Some(input) = rows.next(phase)? {
                    require(consumed < 2_000_000, "execution.group.row_budget")?;
                    consumed += 1;
                    let ordinal = input.ordinal;
                    let (group, decision) = match input.data {
                        GroupInputData::Template(mut template)
                            if phase == GroupPhase::Templates =>
                        {
                            let id = template.id;
                            let rejected = if template.name.is_empty() {
                                Some(GroupLoadDecision::EmptyName)
                            } else if template.kind > 1 {
                                Some(GroupLoadDecision::InvalidType)
                            } else if template.condition != 0
                                && !owner.condition_exists(template.condition)?
                            {
                                Some(GroupLoadDecision::MissingCondition)
                            } else if template.expression != 0
                                && !owner.expression_exists(template.expression)?
                            {
                                Some(GroupLoadDecision::MissingExpression)
                            } else {
                                None
                            };
                            if let Some(decision) = rejected {
                                (id, decision)
                            } else {
                                if template.string_id != 0
                                    && !owner.string_exists(template.string_id)?
                                {
                                    template.string_id = 0;
                                    this.record(
                                        phase,
                                        ordinal,
                                        id,
                                        GroupLoadDecision::StringCleared,
                                    );
                                }
                                definitions.entry(id).or_insert(SpawnGroupDefinition {
                                    template,
                                    ordinal,
                                    formation: None,
                                    members: vec![],
                                    entries: vec![],
                                    equal: vec![],
                                    explicit: vec![],
                                    linked: vec![],
                                    squads: vec![],
                                    active: false,
                                    enabled: true,
                                    chanced: false,
                                });
                                (id, GroupLoadDecision::Accepted)
                            }
                        }
                        GroupInputData::Formation(formation) if phase == GroupPhase::Formations => {
                            let id = formation.group;
                            let decision = if let Some(group) = definitions.get_mut(&id) {
                                if formation.kind >= 7 {
                                    GroupLoadDecision::FormationType
                                } else if formation.movement >= 5 {
                                    GroupLoadDecision::MovementType
                                } else if f32::from_bits(formation.spread_bits) > 15.0
                                    || f32::from_bits(formation.spread_bits) < -15.0
                                {
                                    GroupLoadDecision::FormationSpread
                                } else {
                                    group.formation = Some(formation);
                                    GroupLoadDecision::Accepted
                                }
                            } else {
                                GroupLoadDecision::MissingGroup
                            };
                            (id, decision)
                        }
                        GroupInputData::Member {
                            group: id,
                            guid,
                            slot,
                            chance,
                        } if phase == GroupPhase::Members => {
                            let member = GroupMember {
                                group: id,
                                guid,
                                slot,
                                chance,
                                ordinal,
                                own_entry: 0,
                                random_entry: false,
                            };
                            let decision = if let Some(group) = definitions.get_mut(&id) {
                                let key = SpawnKey {
                                    kind: group.kind(),
                                    guid: member.guid,
                                };
                                if found.contains(&key) {
                                    GroupLoadDecision::DuplicateMember
                                } else if let Some(spawn) = owner.spawn(key)? {
                                    if spawn.excluded() {
                                        GroupLoadDecision::PoolOrEvent
                                    } else {
                                        found.insert(key);
                                        group.chanced |= member.chance != 0;
                                        group.members.push(member);
                                        GroupLoadDecision::Accepted
                                    }
                                } else {
                                    GroupLoadDecision::MissingSpawn
                                }
                            } else {
                                GroupLoadDecision::MissingGroup
                            };
                            (id, decision)
                        }
                        GroupInputData::Entry { group: id, entry }
                            if phase == GroupPhase::Entries =>
                        {
                            let decision = if let Some(group) = definitions.get_mut(&id) {
                                if !owner.template_exists(group.kind(), entry.entry)? {
                                    GroupLoadDecision::MissingTemplate
                                } else {
                                    group.entries.push(entry);
                                    GroupLoadDecision::Accepted
                                }
                            } else {
                                GroupLoadDecision::MissingGroup
                            };
                            (id, decision)
                        }
                        GroupInputData::Link { group: id, linked }
                            if phase == GroupPhase::Links =>
                        {
                            let decision = if !definitions.contains_key(&id) {
                                GroupLoadDecision::MissingGroup
                            } else if !definitions.contains_key(&linked) {
                                GroupLoadDecision::MissingLinkedGroup
                            } else {
                                definitions.get_mut(&id).unwrap().linked.push(linked);
                                GroupLoadDecision::Accepted
                            };
                            (id, decision)
                        }
                        GroupInputData::Squad {
                            group: id,
                            squad,
                            guid,
                            entry,
                        } if phase == GroupPhase::Squads => {
                            let decision = if let Some(group) = definitions.get_mut(&id) {
                                if let Some(existing) =
                                    group.squads.iter_mut().find(|s| s.id == squad)
                                {
                                    existing.entries.entry(guid).or_insert(entry);
                                } else {
                                    group.squads.push(GroupSquad {
                                        id: squad,
                                        entries: BTreeMap::from([(guid, entry)]),
                                    });
                                }
                                GroupLoadDecision::Accepted
                            } else {
                                GroupLoadDecision::MissingGroup
                            };
                            (id, decision)
                        }
                        _ => return Err(refuse("execution.group.loader_phase")),
                    };
                    this.record(phase, ordinal, group, decision);
                }
                // ObjectMgr.cpp:1326-1352 runs after the complete member query.
                if phase == GroupPhase::Members {
                    for (&id, group) in &mut definitions {
                        if group.formation.is_some() {
                            selection::sort_members(&mut group.members);
                            let mut slot = 0_u32;
                            for member in &mut group.members {
                                if member.slot < 0 {
                                    continue;
                                }
                                if member.slot != slot as i32 {
                                    member.slot = slot as i32;
                                    this.record(
                                        phase,
                                        member.ordinal,
                                        id,
                                        GroupLoadDecision::SlotCorrected,
                                    );
                                }
                                slot = slot.wrapping_add(1);
                            }
                        }
                    }
                }
            }
            let mut dynamic = BTreeMap::new();
            for (&id, group) in &mut definitions {
                group.template.max_count = selection::default_max_count(
                    group.template.max_count,
                    &group.entries,
                    group.members.len() as u32,
                );
                for (index, entry) in group.entries.iter().enumerate() {
                    if entry.chance == 0 {
                        group.equal.push(index);
                    } else {
                        group.explicit.push(index);
                    }
                }
                let kind = group.kind();
                for member in &mut group.members {
                    let key = SpawnKey {
                        kind,
                        guid: member.guid,
                    };
                    let spawn = owner
                        .spawn(key)?
                        .ok_or_else(|| refuse("execution.group.final_spawn_absent"))?;
                    owner.detach_spawn(key, spawn)?;
                    this.by_guid.entry(key).or_insert(id);
                    // Deliberately uses creature templates for GO groups too; ObjectMgr.cpp:1532-1544.
                    if owner.autoload_active()? {
                        for entry in &group.entries {
                            if owner
                                .creature_template_flags(entry.entry)?
                                .is_some_and(|flags| flags & 0x1000 != 0)
                            {
                                group.active = true;
                                break;
                            }
                        }
                    }
                    if spawn.spawn_mask == 0 {
                        group.enabled = false;
                    }
                    if spawn.entry != 0 {
                        member.own_entry = spawn.entry;
                    }
                    member.random_entry = owner.has_random_entries(key)?;
                }
                dynamic.insert(
                    id,
                    DynamicSpawnGroup {
                        id,
                        kind,
                        first_guid: group.members.first().map(|m| m.guid),
                        string_id: group.template.string_id,
                    },
                );
            }
            this.definitions = Arc::new(definitions);
            this.dynamic = Arc::new(dynamic);
            this.loaded = true;
            Ok(())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupClockCall {
    Spawn,
    Despawn,
    Vicinity,
    ClearRespawns,
}
#[derive(Clone, Copy, Debug)]
pub struct GroupRemovedObject {
    pub key: SpawnKey,
    pub respawn_delay: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupSpawnRequest {
    pub map: MapIdentity,
    pub thread: u64,
    pub key: SpawnKey,
    pub entry: u32,
}
impl GroupSpawnRequest {
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
        self.binding(context.cells.map(), factory.creature_thread(), attempt)?;
        Ok(matches!(
            spawn_source_creature(factory, attempt, row, self.entry, context)?,
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
        self.binding(context.cells.map(), factory.gameobject_thread(), attempt)?;
        Ok(matches!(
            spawn_source_gameobject(factory, attempt, row, self.entry, context)?,
            Some(GridCandidateOutcome::Loaded(_))
        ))
    }
    fn binding(self, map: MapIdentity, thread: u64, attempt: GridLoadAttempt) -> Result<()> {
        require(
            self.map == map
                && self.map == attempt.grid.map
                && self.thread == thread
                && self.key
                    == SpawnKey {
                        kind: attempt.kind,
                        guid: attempt.guid,
                    },
            "execution.group.constructor_binding",
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GroupCallback {
    FormationCreate,
    FormationUpdate,
    FormationHome,
    FormationRemove {
        object: ObjectHandle,
    },
    SetNoCallAssistance {
        object: ObjectHandle,
    },
    EngageInCombat {
        object: ObjectHandle,
        target: ObjectHandle,
    },
    EnterEvadeMode {
        object: ObjectHandle,
    },
    MemberDied {
        object: ObjectHandle,
        target: Option<ObjectHandle>,
    },
    LastCreatureDespawn {
        object: ObjectHandle,
    },
    ResetLastWaypoint,
    ForcedDespawn {
        object: ObjectHandle,
        delay_millis: u32,
        forced_seconds: u32,
        only_alive: bool,
    },
}
impl GroupCallback {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::FormationCreate
            | Self::FormationUpdate
            | Self::FormationHome
            | Self::FormationRemove { .. }
            | Self::ResetLastWaypoint => "execution.group.f4_13_formation_owner",
            Self::LastCreatureDespawn { .. } => "execution.group.f5_instance_group_despawn_owner",
            Self::SetNoCallAssistance { .. } => "execution.group.f5_assistance_owner",
            Self::EngageInCombat { .. } => "execution.group.f5_combat_engage_owner",
            Self::EnterEvadeMode { .. } => "execution.group.f5_ai_evade_owner",
            Self::MemberDied { .. } => "execution.group.f5_ai_member_died_owner",
            Self::ForcedDespawn { .. } => "execution.group.lifecycle_dispatch_owner",
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum GroupOperation {
    Created(GroupIdentity),
    Enabled {
        group: GroupIdentity,
        enabled: bool,
    },
    Selected {
        group: GroupIdentity,
        guid: u32,
        entry: u32,
    },
    Chance {
        group: GroupIdentity,
        guid: u32,
        result: bool,
        cached: bool,
    },
    Squad {
        group: GroupIdentity,
        index: i32,
    },
    Condition {
        group: GroupIdentity,
        expression: bool,
        result: bool,
    },
    Constructed {
        group: GroupIdentity,
        guid: u32,
        success: bool,
    },
    Cooldown {
        group: GroupIdentity,
        before: i64,
        after: i64,
    },
    Removed {
        group: GroupIdentity,
        guid: u32,
    },
}
/// Conditions, random operations, constructors and callbacks execute; none is a captured answer.
pub trait GroupWork: GroupRandom<Error = Refusal> + PersistentEffects {
    fn thread(&self) -> u64;
    fn wall_seconds(&mut self, call: GroupClockCall) -> Result<ClockSample>;
    fn world_clock_millis(&mut self) -> Result<i64>;
    fn world_game_seconds(&mut self) -> Result<i64>;
    fn condition(
        &mut self,
        groups: &mut SourceSpawnGroups,
        states: &mut SourceManagers,
        group: GroupIdentity,
        id: i32,
        expression: bool,
    ) -> Result<bool>;
    fn roll_chance_i(&mut self, chance: i32) -> Result<bool>;
    fn random_entry(&mut self, key: SpawnKey) -> Result<u32>;
    fn spawn_definition(&mut self, key: SpawnKey) -> Result<Option<GroupSpawnDefinition>>;
    fn is_loaded(&mut self, map: MapIdentity, position_bits: [u32; 3]) -> Result<bool>;
    fn construct(
        &mut self,
        groups: &mut SourceSpawnGroups,
        states: &mut SourceManagers,
        request: GroupSpawnRequest,
    ) -> Result<bool>;
    fn live_object(&mut self, map: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>>;
    fn removed_object(&mut self, object: ObjectHandle) -> Result<GroupRemovedObject>;
    fn has_instance(&mut self, object: ObjectHandle) -> Result<bool>;
    fn callback(
        &mut self,
        _groups: &mut SourceSpawnGroups,
        _states: &mut SourceManagers,
        _group: GroupIdentity,
        callback: GroupCallback,
    ) -> Result<()> {
        Err(callback.missing_owner())
    }
    fn group_operation(&mut self, operation: GroupOperation) -> Result<()>;
}
#[derive(Clone, Debug, Serialize)]
pub struct SpawnGroupsInventory {
    pub version: u32,
    pub loaded: bool,
    pub stopped: bool,
    pub next_generation: u64,
    pub definitions: BTreeMap<u32, SpawnGroupDefinition>,
    pub by_guid: Vec<(SpawnKey, u32)>,
    pub states: Vec<SpawnGroupState>,
    pub records: Vec<GroupLoadRecord>,
    pub frames: Vec<GroupIdentity>,
    pub held_members: Vec<(GroupIdentity, u32)>,
}
impl SpawnGroupsInventory {
    pub fn validate(&self) -> Result<()> {
        require(self.version == 3, "execution.group.inventory_version")?;
        for state in &self.states {
            let definition = self
                .definitions
                .get(&state.identity.handle.id)
                .ok_or_else(|| refuse("execution.group.inventory_definition"))?;
            require(
                state.identity.map.creation_generation > 0
                    && state.identity.handle.generation > 0
                    && state.identity.handle.generation <= self.next_generation
                    && state.persistent.generation > 0
                    && state.identity.map.map == state.persistent.map
                    && state.identity.map.instance == state.persistent.instance,
                "execution.group.inventory_identity",
            )?;
            require(
                state.chosen_squad >= -1
                    && (state.chosen_squad == -1
                        || (state.chosen_squad as usize) < definition.squads.len()),
                "execution.group.inventory_squad",
            )?;
        }
        require(
            self.by_guid.iter().all(|(key, id)| {
                self.definitions.get(id).is_some_and(|d| {
                    d.kind() == key.kind && d.members.iter().any(|m| m.guid == key.guid)
                })
            }),
            "execution.group.inventory_membership",
        )?;
        Ok(())
    }
    pub fn require_quiescent(&self) -> Result<()> {
        self.validate()?;
        require(
            self.loaded
                && !self.stopped
                && self.frames.is_empty()
                && self.held_members.is_empty()
                && self
                    .states
                    .iter()
                    .all(|s| !s.constructing && s.linkage_targets.is_empty()),
            "execution.group.nonquiescent",
        )
    }
    pub fn require_continuation(&self) -> Result<()> {
        self.require_quiescent()?;
        Err(refuse("execution.managers.f7.continuation_owner"))
    }
}
impl SourceSpawnGroups {
    pub fn inventory(&self) -> SpawnGroupsInventory {
        SpawnGroupsInventory {
            version: 3,
            loaded: self.loaded,
            stopped: self.stopped,
            next_generation: self.next_generation,
            definitions: (*self.definitions).clone(),
            by_guid: self
                .by_guid
                .iter()
                .map(|(&key, &group)| (key, group))
                .collect(),
            states: self.states.values().cloned().collect(),
            records: self.records.clone(),
            frames: self.frames.clone(),
            held_members: self.held_members.clone(),
        }
    }
    pub fn state(&self, group: GroupIdentity) -> Result<&SpawnGroupState> {
        self.states
            .get(&(group.map, group.handle.id))
            .filter(|s| s.identity == group)
            .ok_or_else(|| refuse("execution.group.stale_group"))
    }
    pub fn formation_definition(
        &self,
        group: GroupIdentity,
    ) -> Result<(&Formation, &[GroupMember])> {
        self.state(group)?;
        let definition = self.definition(group.handle.id)?;
        Ok((
            definition
                .formation
                .as_ref()
                .ok_or_else(|| refuse("execution.formation.definition_absent"))?,
            &definition.members,
        ))
    }
    pub fn set_formation_data(
        &mut self,
        group: GroupIdentity,
        entry: Option<Formation>,
        formations: &mut SourceFormations,
        owner: &mut impl FormationOwner,
    ) -> Result<()> {
        self.run(|this| {
            this.state(group)?;
            let present = entry.is_some();
            // The new pointer is assigned before Reset; SpawnGroup.cpp:600-601.
            if present {
                this.state_mut(group)?.has_formation = true;
            }
            formations.set_definition(
                group,
                entry,
                &this.definition(group.handle.id)?.members,
                owner,
            )?;
            if !present {
                this.state_mut(group)?.has_formation = false;
            }
            Ok(())
        })
    }
    fn state_mut(&mut self, group: GroupIdentity) -> Result<&mut SpawnGroupState> {
        self.states
            .get_mut(&(group.map, group.handle.id))
            .filter(|s| s.identity == group)
            .ok_or_else(|| refuse("execution.group.stale_group"))
    }
    fn definition(&self, id: u32) -> Result<&SpawnGroupDefinition> {
        self.loaded()?;
        self.definitions
            .get(&id)
            .ok_or_else(|| refuse("execution.group.definition_absent"))
    }
    fn binding(
        &self,
        group: GroupIdentity,
        states: &SourceManagers,
    ) -> Result<PersistentStateIdentity> {
        let state = self.state(group)?;
        let id = state.persistent;
        require(
            states.state(id)?.map_binding() == Some(group.map),
            "execution.group.persistent_map_binding",
        )?;
        Ok(id)
    }
    pub fn create(
        &mut self,
        map: MapIdentity,
        persistent: PersistentStateIdentity,
        id: u32,
        states: &mut SourceManagers,
        work: &mut impl GroupWork,
    ) -> Result<GroupIdentity> {
        self.run(|this| {
            let definition = this.definition(id)?;
            let enabled = definition.enabled;
            let formation =
                definition.kind() == GridSpawnKind::Creature && definition.formation.is_some();
            require(
                map.creation_generation > 0 && states.state(persistent)?.map_binding() == Some(map),
                "execution.group.persistent_map_binding",
            )?;
            require(
                !this.states.contains_key(&(map, id)),
                "execution.group.creation_repeated",
            )?;
            this.next_generation = this
                .next_generation
                .checked_add(1)
                .ok_or_else(|| refuse("execution.group.generation_overflow"))?;
            let group = GroupIdentity {
                map,
                handle: SpawnGroupHandle {
                    id,
                    generation: this.next_generation,
                },
            };
            this.states.insert(
                (map, id),
                SpawnGroupState {
                    identity: group,
                    persistent,
                    objects: BTreeMap::new(),
                    chosen_entries: BTreeMap::new(),
                    chosen_spawns: BTreeMap::new(),
                    chosen_squad: -1,
                    enabled,
                    cooldown_millis: 0,
                    constructing: true,
                    has_formation: formation,
                    linkage_targets: BTreeSet::new(),
                },
            );
            if formation {
                work.callback(this, states, group, GroupCallback::FormationCreate)?;
            }
            this.state_mut(group)?.constructing = false;
            work.group_operation(GroupOperation::Created(group))?;
            Ok(group)
        })
    }
    pub fn group_for(&self, map: MapIdentity, key: SpawnKey) -> Result<Option<GroupIdentity>> {
        self.loaded()?;
        let Some(&id) = self.by_guid.get(&key) else {
            return Ok(None);
        };
        self.states
            .get(&(map, id))
            .map(|s| Some(s.identity))
            .ok_or_else(|| refuse("execution.group.constructor_group_absent"))
    }
    pub fn creature_group(&self, map: MapIdentity, guid: u32) -> Result<Option<CreatureGroup>> {
        self.group_for(
            map,
            SpawnKey {
                kind: GridSpawnKind::Creature,
                guid,
            },
        )?
        .map(|group| {
            let definition = self.definition(group.handle.id)?;
            Ok(CreatureGroup {
                id: group.handle.id,
                string_id: definition.template.string_id,
                has_formation: self.state(group)?.has_formation,
            })
        })
        .transpose()
    }
    pub fn gameobject_group(
        &self,
        map: MapIdentity,
        guid: u32,
    ) -> Result<Option<GameObjectGroupEntry>> {
        self.group_for(
            map,
            SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid,
            },
        )?
        .map(|group| {
            Ok(GameObjectGroupEntry {
                id: group.handle.id,
                string_id: self.definition(group.handle.id)?.template.string_id,
            })
        })
        .transpose()
    }
    pub fn guid_entry(&self, group: GroupIdentity, guid: u32) -> Result<u32> {
        Ok(self.state(group)?.objects.get(&guid).copied().unwrap_or(0))
    }
    pub fn set_enabled(
        &mut self,
        group: GroupIdentity,
        enabled: bool,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.state_mut(group)?.enabled = enabled;
            work.group_operation(GroupOperation::Enabled { group, enabled })
        })
    }
    pub fn add_object(
        &mut self,
        group: GroupIdentity,
        guid: u32,
        entry: u32,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.state_mut(group)?.objects.insert(guid, entry);
            work.group_operation(GroupOperation::Selected { group, guid, entry })
        })
    }
    fn wall(work: &mut impl GroupWork, call: GroupClockCall) -> Result<i64> {
        let sample = work.wall_seconds(call)?;
        require(
            sample.kind() == ClockKind::WallSeconds,
            "execution.group.clock_kind",
        )?;
        Ok(sample.value())
    }
    fn condition_inner(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        work: &mut impl GroupWork,
    ) -> Result<bool> {
        let definition = self.definition(group.handle.id)?;
        let (id, expression) = if definition.template.condition != 0 {
            (definition.template.condition, false)
        } else if definition.template.expression != 0 {
            (definition.template.expression, true)
        } else {
            return Ok(true);
        };
        let result = work.condition(self, states, group, id, expression)?;
        work.group_operation(GroupOperation::Condition {
            group,
            expression,
            result,
        })?;
        Ok(result)
    }
    fn pick_entry(
        definition: &SpawnGroupDefinition,
        member: &GroupMember,
        valid: &mut BTreeMap<u32, u32>,
        minimum: &BTreeMap<u32, u32>,
        work: &mut impl GroupWork,
    ) -> Result<u32> {
        if member.random_entry {
            return work.random_entry(SpawnKey {
                kind: definition.kind(),
                guid: member.guid,
            });
        }
        if member.own_entry != 0 {
            return Ok(member.own_entry);
        }
        selection::eligible_entry(
            &definition.entries,
            &definition.explicit,
            &definition.equal,
            valid,
            minimum,
            work,
        )
        .map_err(|error| match error {
            SelectionError::Owner(error) => error,
            SelectionError::ChanceSubtractUndefined => {
                refuse("execution.group.chance_subtract_undefined")
            }
            SelectionError::RandomIndex => refuse("execution.group.random_index"),
            SelectionError::EntryIndex => refuse("execution.group.entry_index"),
        })
    }
    pub fn spawn(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        forced: bool,
        ignore_respawn: bool,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.frames.push(group);
            this.spawn_inner(group, states, forced, ignore_respawn, work)?;
            this.frames.pop();
            Ok(())
        })
    }
    pub fn update(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.frames.push(group);
            this.spawn_inner(group, states, false, false, work)?;
            if this.state(group)?.has_formation {
                work.callback(this, states, group, GroupCallback::FormationUpdate)?;
            }
            this.frames.pop();
            Ok(())
        })
    }
    fn spawn_inner(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        forced: bool,
        ignore_respawn: bool,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        let persistent = self.binding(group, states)?;
        let definitions = self.definitions.clone();
        let definition = definitions
            .get(&group.handle.id)
            .ok_or_else(|| refuse("execution.group.definition_absent"))?;
        let maximum = definition.template.max_count as usize;
        if !self.state(group)?.enabled && !forced {
            return Ok(());
        }
        if self.state(group)?.cooldown_millis > work.world_clock_millis()? {
            return Ok(());
        }
        // SpawnGroup.cpp:165-181 deliberately checks failed-condition despawn before capacity.
        if definition.template.flags & 8 != 0
            && !self.state(group)?.objects.is_empty()
            && !self.condition_inner(group, states, work)?
        {
            self.despawn_inner(group, states, 0, 1, true, work)?;
            return Ok(());
        }
        if self.state(group)?.objects.len() >= maximum {
            return Ok(());
        }
        if !self.condition_inner(group, states, work)? {
            return Ok(());
        }
        if definition.chanced && self.state(group)?.chosen_spawns.len() >= maximum {
            return Ok(());
        }
        let squad = self.state(group)?.chosen_squad;
        if !definition.squads.is_empty() && squad != -1 {
            let selected = definition
                .squads
                .get(squad as usize)
                .ok_or_else(|| refuse("execution.group.squad_index_undefined"))?;
            if selected.entries.len() == self.state(group)?.objects.len() {
                return Ok(());
            }
        }
        let dungeon = states.state(persistent)?.entry.dungeon();
        let mut eligible = Vec::new();
        let (mut valid, mut minimum) = if definition.squads.is_empty() {
            let state = self.state(group)?;
            for (index, member) in definition.members.iter().enumerate() {
                if (!dungeon
                    || state.chosen_entries.len() != maximum
                    || state.chosen_entries.contains_key(&member.guid))
                    && !state.objects.contains_key(&member.guid)
                {
                    eligible.push(index);
                }
            }
            selection::entry_capacities(&definition.entries, &state.objects)
        } else {
            if !dungeon || squad == -1 {
                let index = work.urand(0, definition.squads.len() as u32 - 1)?;
                require(
                    (index as usize) < definition.squads.len(),
                    "execution.group.random_index",
                )?;
                self.state_mut(group)?.chosen_squad = index as i32;
                work.group_operation(GroupOperation::Squad {
                    group,
                    index: index as i32,
                })?;
            }
            let selected = &definition.squads[self.state(group)?.chosen_squad as usize];
            for (index, member) in definition.members.iter().enumerate() {
                if selected.entries.contains_key(&member.guid) {
                    eligible.push(index);
                }
            }
            (BTreeMap::new(), BTreeMap::new())
        };
        let now = Self::wall(work, GroupClockCall::Spawn)?;
        let kind = definition.kind();
        let mut index = 0;
        while index < eligible.len() {
            let guid = definition.members[eligible[index]].guid;
            if states.state(persistent)?.respawn(kind, guid) > now {
                if !ignore_respawn {
                    if maximum == 1 {
                        return Ok(());
                    }
                    eligible.remove(index);
                    continue;
                }
                let world = work.world_game_seconds()?;
                states.save_respawn(persistent, kind, guid, now, world, work)?;
            }
            index += 1;
        }
        if definition.squads.is_empty() {
            work.shuffle(&mut eligible)?;
            let mut index = 0;
            while index < eligible.len() && self.state(group)?.objects.len() < maximum {
                let member = &definition.members[eligible[index]];
                if definition.chanced {
                    if member.chance != 0 {
                        let cached = self.state(group)?.chosen_spawns.get(&member.guid).copied();
                        let result = if let Some(result) = cached {
                            result
                        } else {
                            let result = work.roll_chance_i(member.chance as i32)?;
                            self.state_mut(group)?
                                .chosen_spawns
                                .insert(member.guid, result);
                            result
                        };
                        work.group_operation(GroupOperation::Chance {
                            group,
                            guid: member.guid,
                            result,
                            cached: cached.is_some(),
                        })?;
                        if !result {
                            eligible.remove(index);
                            continue;
                        }
                    } else {
                        self.state_mut(group)?
                            .chosen_spawns
                            .insert(member.guid, true);
                    }
                }
                index += 1;
            }
            // C++ subtracts size_t, including any defined reentrant growth after the earlier check.
            eligible.truncate(
                eligible
                    .len()
                    .min(maximum.wrapping_sub(self.state(group)?.objects.len())),
            );
            if dungeon && kind == GridSpawnKind::Creature {
                for own in [true, false] {
                    for &index in &eligible {
                        let member = &definition.members[index];
                        if (member.random_entry || member.own_entry != 0) == own {
                            let entry =
                                Self::pick_entry(definition, member, &mut valid, &minimum, work)?;
                            self.state_mut(group)?
                                .chosen_entries
                                .insert(member.guid, entry);
                            selection::erase_entry(entry, &mut valid, &mut minimum);
                        }
                    }
                }
            }
        } else if dungeon && kind == GridSpawnKind::Creature {
            let squad = &definition.squads[self.state(group)?.chosen_squad as usize];
            for (&guid, &entry) in &squad.entries {
                self.state_mut(group)?.chosen_entries.insert(guid, entry);
            }
        }
        for index in eligible {
            let member = &definition.members[index];
            let entry = if dungeon && kind == GridSpawnKind::Creature {
                *self
                    .state_mut(group)?
                    .chosen_entries
                    .entry(member.guid)
                    .or_default()
            } else if self.state(group)?.chosen_squad == -1 {
                let entry = Self::pick_entry(definition, member, &mut valid, &minimum, work)?;
                selection::erase_entry(entry, &mut valid, &mut minimum);
                entry
            } else {
                *definition
                    .squads
                    .get(self.state(group)?.chosen_squad as usize)
                    .and_then(|s| s.entries.get(&member.guid))
                    .ok_or_else(|| refuse("execution.group.squad_member_undefined"))?
            };
            let key = SpawnKey {
                kind,
                guid: member.guid,
            };
            let data = work
                .spawn_definition(key)?
                .ok_or_else(|| refuse("execution.group.spawn_definition_absent"))?;
            states.overlay(persistent, data.cell, kind, member.guid, true, work)?;
            self.add_object(group, member.guid, entry, work)?;
            if forced || definition.active || work.is_loaded(group.map, data.position_bits)? {
                let request = GroupSpawnRequest {
                    map: group.map,
                    thread: work.thread(),
                    key,
                    entry,
                };
                let success = work.construct(self, states, request)?;
                work.group_operation(GroupOperation::Constructed {
                    group,
                    guid: member.guid,
                    success,
                })?;
            }
        }
        Ok(())
    }
    pub fn despawn(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        delay_millis: u32,
        forced_seconds: u32,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.frames.push(group);
            this.despawn_inner(group, states, delay_millis, forced_seconds, true, work)?;
            this.frames.pop();
            Ok(())
        })
    }
    // Explicit creature overload preserves onlyAlive in delayed events; SpawnGroup.h:97-98.
    pub fn despawn_creatures(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        delay_millis: u32,
        only_alive: bool,
        forced_seconds: u32,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            require(
                this.definition(group.handle.id)?.kind() == GridSpawnKind::Creature,
                "execution.group.creature_group_required",
            )?;
            this.frames.push(group);
            this.despawn_inner(
                group,
                states,
                delay_millis,
                forced_seconds,
                only_alive,
                work,
            )?;
            this.frames.pop();
            Ok(())
        })
    }
    fn despawn_inner(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        delay_millis: u32,
        forced_seconds: u32,
        only_alive: bool,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        let persistent = self.binding(group, states)?;
        let kind = self.definition(group.handle.id)?.kind();
        let when = Self::wall(work, GroupClockCall::Despawn)?
            .checked_add(i64::from(forced_seconds))
            .ok_or_else(|| refuse("execution.group.deadline_add_undefined"))?;
        let objects = self.state(group)?.objects.clone();
        for &guid in objects.keys() {
            let key = SpawnKey { kind, guid };
            if let Some(object) = work.live_object(group.map, key)? {
                work.callback(
                    self,
                    states,
                    group,
                    GroupCallback::ForcedDespawn {
                        object,
                        delay_millis,
                        forced_seconds,
                        only_alive: kind == GridSpawnKind::Creature && only_alive,
                    },
                )?;
            } else if delay_millis == 0 {
                let data = work
                    .spawn_definition(key)?
                    .ok_or_else(|| refuse("execution.group.spawn_definition_absent"))?;
                states.overlay(persistent, data.cell, kind, guid, false, work)?;
                let world = work.world_game_seconds()?;
                states.save_respawn(persistent, kind, guid, when, world, work)?;
            }
        }
        if delay_millis == 0 {
            self.require_member_erase(group, None)?;
            self.state_mut(group)?.objects.clear();
        }
        Ok(())
    }
    pub fn is_respawn_overridden(&self, group: GroupIdentity) -> Result<bool> {
        self.state(group)?;
        Ok(self
            .definition(group.handle.id)?
            .template
            .respawn_min
            .is_some())
    }
    pub fn random_respawn_time(
        &self,
        group: GroupIdentity,
        work: &mut impl GroupWork,
    ) -> Result<u32> {
        self.state(group)?;
        let template = &self.definition(group.handle.id)?.template;
        let minimum = template
            .respawn_min
            .ok_or_else(|| refuse("execution.group.respawn_override_absent_undefined"))?;
        if let Some(maximum) = template.respawn_max {
            work.urand(minimum, maximum)
        } else {
            Ok(minimum)
        }
    }
    /// SpawnGroup.cpp:439-471 samples time before checking whether any member is near.
    pub fn respawn_if_in_vicinity(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        position_bits: [u32; 4],
        range_bits: u32,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.frames.push(group);
            let persistent = this.binding(group, states)?;
            if this.condition_inner(group, states, work)? {
                let now = Self::wall(work, GroupClockCall::Vicinity)?;
                let definitions = this.definitions.clone();
                let definition = definitions
                    .get(&group.handle.id)
                    .ok_or_else(|| refuse("execution.group.definition_absent"))?;
                let kind = definition.kind();
                let mut eligible = false;
                for member in &definition.members {
                    let data = work
                        .spawn_definition(SpawnKey {
                            kind,
                            guid: member.guid,
                        })?
                        .ok_or_else(|| refuse("execution.group.spawn_definition_absent"))?;
                    if selection::in_vicinity(position_bits, data.position_bits, range_bits) {
                        eligible = true;
                        break;
                    }
                }
                if eligible {
                    for member in &definition.members {
                        let world = work.world_game_seconds()?;
                        states.save_respawn(persistent, kind, member.guid, now, world, work)?;
                    }
                }
            }
            this.frames.pop();
            Ok(())
        })
    }
    /// CreatureGroup::ClearRespawnTimes, SpawnGroup.cpp:692-696.
    pub fn clear_respawn_times(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.frames.push(group);
            let persistent = this.binding(group, states)?;
            require(
                this.definition(group.handle.id)?.kind() == GridSpawnKind::Creature,
                "execution.group.creature_group_required",
            )?;
            let now = Self::wall(work, GroupClockCall::ClearRespawns)?;
            let definitions = this.definitions.clone();
            for member in &definitions[&group.handle.id].members {
                let world = work.world_game_seconds()?;
                states.save_respawn(
                    persistent,
                    GridSpawnKind::Creature,
                    member.guid,
                    now,
                    world,
                    work,
                )?;
            }
            this.frames.pop();
            Ok(())
        })
    }
    pub fn remove_object(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        object: ObjectHandle,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        self.run(|this| {
            this.frames.push(group);
            let persistent = this.binding(group, states)?;
            let definitions = this.definitions.clone();
            let definition = definitions
                .get(&group.handle.id)
                .ok_or_else(|| refuse("execution.group.definition_absent"))?;
            let removed = work.removed_object(object)?;
            require(
                removed.key.kind == definition.kind(),
                "execution.group.removed_object_kind",
            )?;
            this.require_member_erase(group, Some(removed.key.guid))?;
            this.state_mut(group)?.objects.remove(&removed.key.guid);
            if !states.state(persistent)?.entry.dungeon() && this.state(group)?.objects.is_empty() {
                let before = this.state(group)?.cooldown_millis;
                let delay = if let Some(minimum) = definition.template.respawn_min {
                    Some(work.urand(minimum, definition.template.respawn_max.unwrap_or(minimum))?)
                } else if this.state(group)?.chosen_squad != -1 {
                    Some(removed.respawn_delay)
                } else {
                    None
                };
                if let Some(delay) = delay {
                    let now = work.world_clock_millis()?;
                    let after = now
                        .checked_add(i64::from(delay) * 1000)
                        .ok_or_else(|| refuse("execution.group.cooldown_add_undefined"))?;
                    this.state_mut(group)?.cooldown_millis = after;
                    work.group_operation(GroupOperation::Cooldown {
                        group,
                        before,
                        after,
                    })?;
                }
                this.state_mut(group)?.chosen_squad = -1;
                if definition.chanced {
                    this.state_mut(group)?.chosen_spawns.clear();
                    let deadline = states
                        .state(persistent)?
                        .respawn(removed.key.kind, removed.key.guid);
                    for member in &definition.members {
                        if member.guid != removed.key.guid {
                            let world = work.world_game_seconds()?;
                            states.save_respawn(
                                persistent,
                                removed.key.kind,
                                member.guid,
                                deadline,
                                world,
                                work,
                            )?;
                        }
                    }
                }
            }
            let data = work
                .spawn_definition(removed.key)?
                .ok_or_else(|| refuse("execution.group.spawn_definition_absent"))?;
            states.overlay(
                persistent,
                data.cell,
                removed.key.kind,
                removed.key.guid,
                false,
                work,
            )?;
            if removed.key.kind == GridSpawnKind::Creature
                && this.state(group)?.objects.is_empty()
                && work.has_instance(object)?
            {
                work.callback(
                    this,
                    states,
                    group,
                    GroupCallback::LastCreatureDespawn { object },
                )?;
                if this.state(group)?.has_formation {
                    work.callback(this, states, group, GroupCallback::ResetLastWaypoint)?;
                }
            }
            work.group_operation(GroupOperation::Removed {
                group,
                guid: removed.key.guid,
            })?;
            this.frames.pop();
            Ok(())
        })
    }
}
