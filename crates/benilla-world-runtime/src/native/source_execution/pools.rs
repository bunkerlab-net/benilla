// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! PoolManager loading, selection and synchronous leaf effects, without production admission.
use super::super::production_compiler::{PoolCatalogRows, VerifiedCatalog};
use super::*;
use benilla_sim::source_pools::{self, PoolGroup, PoolObject, RollContext};
use std::collections::{BTreeMap, BTreeSet};
mod effects;
mod maps;
pub use effects::*;
pub use maps::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum PoolKind {
    Child,
    GameObject,
    Creature,
}
impl PoolKind {
    fn index(self) -> usize {
        match self {
            Self::Child => 0,
            Self::GameObject => 1,
            Self::Creature => 2,
        }
    }
    fn member(self, guid: u32) -> PoolMember {
        match self {
            Self::Child => PoolMember::Pool(guid),
            Self::GameObject => PoolMember::GameObject(guid),
            Self::Creature => PoolMember::Creature(guid),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolTemplateRow {
    pub ordinal: u64,
    pub id: u16,
    pub limit: u32,
    pub description: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoolMembershipRow {
    pub ordinal: u64,
    pub guid: u32,
    pub pool: u16,
    pub chance: f32,
}
pub trait PoolRows {
    fn maximum(&mut self) -> Result<Option<u16>>;
    fn template(&mut self) -> Result<Option<PoolTemplateRow>>;
    /// Source query slots 2..=6: creature GUID/template, GO GUID/template, child pools.
    fn member(&mut self, slot: usize) -> Result<Option<PoolMembershipRow>>;
}
impl PoolRows for PoolCatalogRows {
    fn maximum(&mut self) -> Result<Option<u16>> {
        Ok(self.maximum()?)
    }
    fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
        Ok(self.template()?.map(|v| PoolTemplateRow {
            ordinal: v.ordinal,
            id: v.id,
            limit: v.limit,
            description: v.description,
        }))
    }
    fn member(&mut self, slot: usize) -> Result<Option<PoolMembershipRow>> {
        Ok(self.member(slot)?.map(|v| PoolMembershipRow {
            ordinal: v.ordinal,
            guid: v.guid,
            pool: v.pool,
            chance: v.chance,
        }))
    }
}
/// Effective ObjectMgr definitions, not raw catalog spawn presence or F2 candidates.
pub trait PoolLoadOwner {
    fn spawn_map(&mut self, kind: PoolKind, guid: u32) -> Result<Option<u32>>;
    fn map_exists(&mut self, map: u32) -> Result<bool>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PoolEffectAction {
    Spawn { instantly: bool },
    Despawn,
    Respawn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PoolLeafEffect {
    pub state: PersistentStateIdentity,
    pub kind: PoolKind,
    pub guid: u32,
    pub action: PoolEffectAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PoolPending {
    EffectiveDefinitions,
    RandomOwner,
    LeafEffects,
    SealedExecution,
    DurableContinuation,
}
impl PoolPending {
    pub fn refusal(self) -> Refusal {
        refuse(match self {
            Self::EffectiveDefinitions => "execution.pool.effective_definitions_owner",
            Self::RandomOwner => "execution.pool.random_owner",
            Self::LeafEffects => "execution.pool.leaf_effect_owner",
            Self::SealedExecution => "execution.pool.sealed_occurrence_binding_pending",
            Self::DurableContinuation => "execution.managers.f7_continuation_owner",
        })
    }
}
/// Every reached method executes its owner. Captured outcomes are not inputs.
pub trait PoolWork {
    fn engine_access(&mut self) -> Result<()>;
    fn chance(&mut self) -> Result<f64>;
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<()>;
    fn can_spawn(
        &mut self,
        manager: &mut SourcePoolManager,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        kind: PoolKind,
        guid: u32,
    ) -> Result<bool>;
    fn leaf_effect(
        &mut self,
        manager: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()>;
    fn observe(&mut self, operation: PoolOperation) -> Result<()>;
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PoolTemplate {
    pub map: Option<u32>,
    pub limit: u32,
    pub auto_spawn: bool,
    pub description: Vec<u8>,
    pub groups: [PoolGroup; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PoolLoadDecision {
    Accepted,
    MissingSpawn,
    PoolOutOfRange,
    ChildOutOfRange,
    SelfCycle,
    InvalidChance,
    DuplicateGuid,
    MissingMap,
    DifferentMap,
    CycleRemoved,
    RelationMapRemoved,
    IntegrityDisabled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolLoadRecord {
    pub slot: usize,
    pub ordinal: u64,
    pub guid: u32,
    pub pool: u16,
    pub decision: PoolLoadDecision,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PoolFrame {
    pub state: PersistentStateIdentity,
    pub pool: u16,
    pub kind: PoolKind,
    pub count: i32,
    pub iteration: i32,
    pub trigger: u32,
    pub last_despawned: u32,
    pub instantly: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PoolOperation {
    Selected {
        state: PersistentStateIdentity,
        pool: u16,
        kind: PoolKind,
        guid: u32,
        add: bool,
        count: u32,
    },
    Roll {
        state: PersistentStateIdentity,
        pool: u16,
        kind: PoolKind,
        trigger: u32,
        guid: Option<u32>,
    },
    Leaf(PoolLeafEffect),
    ChildRespawnNoop {
        state: PersistentStateIdentity,
        pool: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolInventory {
    pub version: u32,
    pub loaded: bool,
    pub stopped: bool,
    pub maximum: u16,
    pub templates: Vec<PoolTemplate>,
    pub membership: [BTreeMap<u32, u16>; 3],
    pub records: Vec<PoolLoadRecord>,
    pub frames: Vec<PoolFrame>,
}
impl PoolInventory {
    pub fn require_quiescent(&self) -> Result<()> {
        require(
            self.version == 1 && self.loaded && !self.stopped && self.frames.is_empty(),
            "execution.pool.nonquiescent",
        )?;
        require(
            self.templates.len() <= usize::from(self.maximum) + 1
                && self.membership.iter().all(|map| {
                    map.values()
                        .all(|id| usize::from(*id) < self.templates.len())
                }),
            "execution.pool.inventory",
        )
    }
    pub fn require_durable_checkpoint(&self) -> Result<()> {
        Err(PoolPending::DurableContinuation.refusal())
    }
}
/// Only LoadFromDB's MAX phase, not loaded templates or memberships.
#[derive(Default)]
pub struct SourcePoolMaximum {
    maximum: Option<u16>,
    query_returned: bool,
}
impl SourcePoolMaximum {
    pub fn load(rows: &mut impl PoolRows) -> Result<Self> {
        // PoolManager.cpp:566,606-614; Field.h:57 narrows before storage.
        let maximum = rows.maximum()?;
        Ok(Self {
            maximum: Some(maximum.unwrap_or(0)),
            query_returned: maximum.is_some(),
        })
    }
    pub fn load_catalog(catalog: &mut VerifiedCatalog, occurrences: &Path) -> Result<Self> {
        Self::load(&mut catalog.execution_pool_rows(occurrences)?)
    }
    pub fn maximum(&self) -> Result<u16> {
        self.maximum
            .ok_or_else(|| refuse("execution.db_scripts.pool_max_id_owner"))
    }
}
impl DbScriptLoaderOwner for SourcePoolMaximum {
    fn max_pool_id(&mut self) -> Result<u32> {
        self.maximum().map(u32::from)
    }
}

#[derive(Default)]
pub struct SourcePoolManager {
    maximum: u16,
    templates: Vec<PoolTemplate>,
    membership: [BTreeMap<u32, u16>; 3],
    records: Vec<PoolLoadRecord>,
    frames: Vec<PoolFrame>,
    loaded: bool,
    started: bool,
    stopped: bool,
}
impl DbScriptLoaderOwner for SourcePoolManager {
    fn max_pool_id(&mut self) -> Result<u32> {
        require(
            self.loaded && !self.stopped,
            "execution.db_scripts.pool_max_id_owner",
        )?;
        // PoolManager.h:140,193 returns stored uint16, not the last template.
        Ok(u32::from(self.maximum))
    }
}
fn numeric(error: source_pools::Error) -> Refusal {
    refuse(match error {
        source_pools::Error::CountIncrementUndefined => "execution.pool.count_increment_undefined",
        source_pools::Error::TemplateLoopUnbounded => "execution.pool.template_loop_unbounded",
    })
}
impl SourcePoolManager {
    pub fn inventory(&self) -> PoolInventory {
        PoolInventory {
            version: 1,
            loaded: self.loaded,
            stopped: self.stopped,
            maximum: self.maximum,
            templates: self.templates.clone(),
            membership: self.membership.clone(),
            records: self.records.clone(),
            frames: self.frames.clone(),
        }
    }
    fn run<T>(&mut self, body: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        require(!self.stopped, "execution.pool.stopped")?;
        let result = body(self);
        self.stopped |= result.is_err();
        result
    }
    fn ensure_loaded(&self) -> Result<()> {
        require(self.loaded && !self.stopped, "execution.pool.loader_owner")
    }
    fn template(&self, id: u16) -> Result<&PoolTemplate> {
        self.templates
            .get(usize::from(id))
            .ok_or_else(|| refuse("execution.pool.template_index_undefined"))
    }
    pub fn part_of(&self, kind: PoolKind, guid: u32) -> Result<u16> {
        self.ensure_loaded()?;
        Ok(self.membership[kind.index()]
            .get(&guid)
            .copied()
            .unwrap_or(0))
    }
    pub fn pool_template(&self, pool: u16) -> Result<&PoolTemplate> {
        self.ensure_loaded()?;
        self.template(pool)
    }
    pub fn check_pool(&self, pool: u16) -> Result<bool> {
        self.ensure_loaded()?;
        if pool > self.maximum {
            return Ok(false);
        }
        let template = self.template(pool)?;
        Ok([PoolKind::GameObject, PoolKind::Creature, PoolKind::Child]
            .into_iter()
            .all(|kind| template.groups[kind.index()].valid_chances()))
    }
    pub fn top_pool(&self, kind: PoolKind, guid: u32) -> Result<u16> {
        let mut pool = self.part_of(kind, guid)?;
        while pool != 0 {
            let parent = self.part_of(PoolKind::Child, u32::from(pool))?;
            if parent == 0 {
                break;
            }
            pool = parent;
        }
        Ok(pool)
    }
    pub fn remove_auto_spawn(&mut self, pool: u16) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            this.template(pool)?;
            this.templates[usize::from(pool)].auto_spawn = false;
            Ok(())
        })
    }
    pub fn set_excluded(
        &mut self,
        kind: PoolKind,
        pool: u16,
        guid: u32,
        excluded: bool,
    ) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            this.template(pool)?;
            this.templates[usize::from(pool)].groups[kind.index()].exclude(guid, excluded);
            Ok(())
        })
    }
    pub fn load_catalog(
        &mut self,
        catalog: &mut VerifiedCatalog,
        occurrences: &Path,
        owner: &mut impl PoolLoadOwner,
    ) -> Result<()> {
        self.run(|this| {
            let mut rows = catalog.execution_pool_rows(occurrences)?;
            this.load(&mut rows, owner)
        })
    }
    pub fn load(&mut self, rows: &mut impl PoolRows, owner: &mut impl PoolLoadOwner) -> Result<()> {
        self.run(|this| {
            require(!this.started, "execution.pool.loader_repeated")?;
            this.started = true;
            let maximum = SourcePoolMaximum::load(rows)?;
            this.maximum = maximum.maximum()?;
            if !maximum.query_returned {
                this.loaded = true;
                return Ok(());
            }
            let maximum = this.maximum;
            this.templates
                .resize_with(usize::from(maximum) + 1, PoolTemplate::default);
            let mut templates = 0;
            while let Some(row) = rows.template()? {
                this.template(row.id)?;
                this.templates[usize::from(row.id)] = PoolTemplate {
                    limit: row.limit,
                    auto_spawn: true,
                    description: row.description,
                    ..PoolTemplate::default()
                };
                this.record(
                    1,
                    row.ordinal,
                    u32::from(row.id),
                    row.id,
                    PoolLoadDecision::Accepted,
                )?;
                templates += 1;
            }
            if templates == 0 {
                this.templates.clear();
                this.loaded = true;
                return Ok(());
            }
            for slot in 2..=6 {
                while let Some(row) = rows.member(slot)? {
                    this.load_member(slot, row, owner)?;
                }
            }
            for start in 0..maximum {
                let mut child = start;
                let mut checked = BTreeSet::new();
                while let Some(parent) = this.membership[0].get(&u32::from(child)).copied() {
                    let map_error = if let Some(map) = this.template(child)?.map {
                        this.map_check(parent, map, owner)?
                    } else {
                        None
                    };
                    checked.insert(child);
                    let reason = if map_error.is_some() {
                        Some(PoolLoadDecision::RelationMapRemoved)
                    } else if checked.contains(&parent) {
                        Some(PoolLoadDecision::CycleRemoved)
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        this.templates[usize::from(parent)].groups[0]
                            .remove_relation(u32::from(child));
                        this.membership[0].remove(&u32::from(child));
                        this.record(6, u64::from(start), u32::from(child), parent, reason)?;
                        break;
                    }
                    child = parent;
                }
            }
            source_pools::template_loop(maximum).map_err(numeric)?;
            for id in 0..=maximum {
                let template = &mut this.templates[usize::from(id)];
                if template.auto_spawn
                    && ![PoolKind::GameObject, PoolKind::Creature, PoolKind::Child]
                        .into_iter()
                        .all(|kind| template.groups[kind.index()].valid_chances())
                {
                    template.auto_spawn = false;
                    this.record(
                        7,
                        u64::from(id),
                        u32::from(id),
                        id,
                        PoolLoadDecision::IntegrityDisabled,
                    )?;
                }
            }
            this.loaded = true;
            Ok(())
        })
    }
    fn record(
        &mut self,
        slot: usize,
        ordinal: u64,
        guid: u32,
        pool: u16,
        decision: PoolLoadDecision,
    ) -> Result<()> {
        require(self.records.len() < 2_000_000, "execution.pool.row_budget")?;
        self.records.push(PoolLoadRecord {
            slot,
            ordinal,
            guid,
            pool,
            decision,
        });
        Ok(())
    }
    fn map_check(
        &mut self,
        pool: u16,
        map: u32,
        owner: &mut impl PoolLoadOwner,
    ) -> Result<Option<PoolLoadDecision>> {
        if !owner.map_exists(map)? {
            return Ok(Some(PoolLoadDecision::MissingMap));
        }
        let template = &mut self.templates[usize::from(pool)];
        if template.map.is_some_and(|value| value != map) {
            return Ok(Some(PoolLoadDecision::DifferentMap));
        }
        template.map = Some(map);
        Ok(None)
    }
    fn load_member(
        &mut self,
        slot: usize,
        mut row: PoolMembershipRow,
        owner: &mut impl PoolLoadOwner,
    ) -> Result<()> {
        let kind = match slot {
            2 | 3 => PoolKind::Creature,
            4 | 5 => PoolKind::GameObject,
            _ => PoolKind::Child,
        };
        if kind == PoolKind::Child {
            row.guid = u32::from(row.guid as u16);
        }
        let mut map = None;
        let mut reason = None;
        if kind != PoolKind::Child {
            map = owner.spawn_map(kind, row.guid)?;
            if map.is_none() {
                reason = Some(PoolLoadDecision::MissingSpawn);
            }
        }
        if reason.is_none() {
            reason = if row.pool > self.maximum {
                Some(PoolLoadDecision::PoolOutOfRange)
            } else if kind == PoolKind::Child && row.guid > u32::from(self.maximum) {
                Some(PoolLoadDecision::ChildOutOfRange)
            } else if kind == PoolKind::Child && row.guid == u32::from(row.pool) {
                Some(PoolLoadDecision::SelfCycle)
            } else if row.chance < 0.0 || row.chance > 100.0 {
                Some(PoolLoadDecision::InvalidChance)
            } else {
                None
            };
        }
        if reason.is_none()
            && matches!(slot, 3 | 5)
            && self.membership[kind.index()]
                .get(&row.guid)
                .is_some_and(|v| *v != 0)
        {
            reason = Some(PoolLoadDecision::DuplicateGuid);
        }
        if reason.is_none() {
            if let Some(map) = map {
                reason = self.map_check(row.pool, map, owner)?;
            }
        }
        if reason.is_none() {
            let template = &mut self.templates[usize::from(row.pool)];
            template.groups[kind.index()].add(
                PoolObject::new(row.guid, row.chance, row.ordinal),
                template.limit,
            );
            self.membership[kind.index()]
                .entry(row.guid)
                .or_insert(row.pool);
            if kind == PoolKind::Child {
                self.templates[row.guid as usize].auto_spawn = false;
            }
        }
        self.record(
            slot,
            row.ordinal,
            row.guid,
            row.pool,
            reason.unwrap_or(PoolLoadDecision::Accepted),
        )
    }
    pub fn initialize(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            if !this.templates.is_empty() {
                source_pools::template_loop(this.maximum).map_err(numeric)?;
            }
            let map = states.state(state)?.entry.id;
            for index in 0..this.templates.len() {
                let template = &this.templates[index];
                if template.auto_spawn && template.map == Some(map) {
                    this.spawn_inner(states, state, index as u16, true, work)?;
                }
            }
            Ok(())
        })
    }
    pub fn spawn_pool(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        instantly: bool,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            this.spawn_inner(states, state, pool, instantly, work)
        })
    }
    fn spawn_inner(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        instantly: bool,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        for kind in [PoolKind::Child, PoolKind::GameObject, PoolKind::Creature] {
            self.spawn_group(states, state, pool, kind, 0, instantly, work)?;
        }
        Ok(())
    }
    pub fn update_pool(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        kind: PoolKind,
        trigger: u32,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            let parent = this.part_of(PoolKind::Child, u32::from(pool))?;
            if parent != 0 {
                this.spawn_group(
                    states,
                    state,
                    parent,
                    PoolKind::Child,
                    u32::from(pool),
                    false,
                    work,
                )
            } else {
                this.spawn_group(states, state, pool, kind, trigger, false, work)
            }
        })
    }
    fn change_selection(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        member: PoolMember,
        add: bool,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        let (kind, guid) = match member {
            PoolMember::Creature(guid) => (PoolKind::Creature, guid),
            PoolMember::GameObject(guid) => (PoolKind::GameObject, guid),
            PoolMember::Pool(guid) => (PoolKind::Child, guid),
        };
        let data = states.state_mut(state)?.pools_mut();
        if add {
            data.add(member, u32::from(pool));
        } else {
            data.remove(member, u32::from(pool));
        }
        work.observe(PoolOperation::Selected {
            state,
            pool,
            kind,
            guid,
            add,
            count: data.count(u32::from(pool)),
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn spawn_group(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        kind: PoolKind,
        mut trigger: u32,
        instantly: bool,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        let template = self.template(pool)?;
        let group = &template.groups[kind.index()];
        if group.is_empty() {
            return Ok(());
        }
        let data = states.state(state)?.pools();
        let selected = trigger != 0 && data.contains(kind.member(trigger));
        let count =
            source_pools::spawn_count(template.limit, data.count(u32::from(pool)), selected)
                .map_err(numeric)?;
        if !selected {
            trigger = 0;
        }
        let frame = self.frames.len();
        self.frames.push(PoolFrame {
            state,
            pool,
            kind,
            count,
            iteration: 0,
            trigger,
            last_despawned: 0,
            instantly,
        });
        for iteration in 0..count {
            self.frames[frame].iteration = iteration;
            let trigger = self.frames[frame].trigger;
            let group = &self.template(pool)?.groups[kind.index()];
            let (explicit, equal) = (group.explicit.len(), group.equal.len());
            let object = source_pools::roll_one(
                explicit,
                equal,
                trigger,
                &mut PoolRoll {
                    manager: self,
                    states,
                    state,
                    pool,
                    kind,
                    work,
                },
            )?;
            work.observe(PoolOperation::Roll {
                state,
                pool,
                kind,
                trigger,
                guid: object.map(|v| v.guid),
            })?;
            let Some(object) = object else { continue };
            if object.guid == self.frames[frame].last_despawned {
                continue;
            }
            if object.guid == trigger {
                let data = states.state(state)?.pools();
                require(
                    data.contains(kind.member(object.guid)) && data.count(u32::from(pool)) > 0,
                    "execution.pool.respawn_assertion",
                )?;
                self.effect(
                    states,
                    state,
                    kind,
                    object.guid,
                    PoolEffectAction::Respawn,
                    work,
                )?;
                self.frames[frame].trigger = 0;
                continue;
            }
            self.change_selection(states, state, pool, kind.member(object.guid), true, work)?;
            self.effect(
                states,
                state,
                kind,
                object.guid,
                PoolEffectAction::Spawn { instantly },
                work,
            )?;
            if trigger != 0 {
                self.despawn_group(states, state, pool, kind, trigger, work)?;
                self.frames[frame].last_despawned = trigger;
                self.frames[frame].trigger = 0;
            }
        }
        self.frames.pop();
        Ok(())
    }
    fn effect(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        kind: PoolKind,
        guid: u32,
        action: PoolEffectAction,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        if kind == PoolKind::Child {
            return match action {
                PoolEffectAction::Spawn { instantly } => {
                    self.spawn_inner(states, state, guid as u16, instantly, work)
                }
                PoolEffectAction::Despawn => self.despawn_inner(states, state, guid as u16, work),
                PoolEffectAction::Respawn => {
                    work.observe(PoolOperation::ChildRespawnNoop { state, pool: guid })
                }
            };
        }
        let effect = PoolLeafEffect {
            state,
            kind,
            guid,
            action,
        };
        work.leaf_effect(self, states, effect)?;
        work.observe(PoolOperation::Leaf(effect))
    }
    pub fn despawn_pool(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            this.despawn_inner(states, state, pool, work)
        })
    }
    fn despawn_inner(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        for kind in [PoolKind::Creature, PoolKind::GameObject, PoolKind::Child] {
            self.despawn_group(states, state, pool, kind, 0, work)?;
        }
        Ok(())
    }
    fn despawn_group(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        kind: PoolKind,
        guid: u32,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        for explicit in [false, true] {
            let count = {
                let group = &self.template(pool)?.groups[kind.index()];
                if explicit {
                    group.explicit.len()
                } else {
                    group.equal.len()
                }
            };
            for index in 0..count {
                let group = &self.template(pool)?.groups[kind.index()];
                let member = if explicit {
                    group.explicit[index]
                } else {
                    group.equal[index]
                };
                if states
                    .state(state)?
                    .pools()
                    .contains(kind.member(member.guid))
                    && (guid == 0 || member.guid == guid)
                {
                    self.effect(
                        states,
                        state,
                        kind,
                        member.guid,
                        PoolEffectAction::Despawn,
                        work,
                    )?;
                    self.change_selection(
                        states,
                        state,
                        pool,
                        kind.member(member.guid),
                        false,
                        work,
                    )?;
                }
            }
        }
        Ok(())
    }
}
struct PoolRoll<'a, W> {
    manager: &'a mut SourcePoolManager,
    states: &'a mut SourceManagers,
    state: PersistentStateIdentity,
    pool: u16,
    kind: PoolKind,
    work: &'a mut W,
}
impl<W: PoolWork> RollContext for PoolRoll<'_, W> {
    type Error = Refusal;
    fn engine_access(&mut self) -> Result<()> {
        self.work.engine_access()
    }
    fn chance(&mut self) -> Result<f64> {
        self.work.chance()
    }
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<()> {
        self.work.shuffle(indices)
    }
    fn object(&self, explicit: bool, index: usize) -> PoolObject {
        let group = &self.manager.templates[usize::from(self.pool)].groups[self.kind.index()];
        if explicit {
            group.explicit[index]
        } else {
            group.equal[index]
        }
    }
    fn can_spawn(&mut self, guid: u32) -> Result<bool> {
        if self.kind == PoolKind::Child || self.states.state(self.state)?.map_binding().is_none() {
            return Ok(true);
        }
        let allowed =
            self.work
                .can_spawn(self.manager, self.states, self.state, self.kind, guid)?;
        // PoolManager.cpp:368-381 discards Creature's boolean, never an owner refusal.
        Ok(self.kind == PoolKind::Creature || allowed)
    }
    fn selected(&self, guid: u32) -> Result<bool> {
        Ok(self
            .states
            .state(self.state)?
            .pools()
            .contains(self.kind.member(guid)))
    }
}
