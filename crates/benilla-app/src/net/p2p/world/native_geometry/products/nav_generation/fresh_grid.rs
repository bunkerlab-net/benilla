// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded fresh EnsureGridCreated ownership, not Map::Initialize or registration.
use super::super::source_query::SourceMapQuerySnapshot;
use super::{
    positive_load::{PositiveTileLoad, VmapLoadConfiguration},
    *,
};
use benilla_native_nav::{RetainedSourceQuery, SourceMapQuerySlot};
use benilla_sim::resident_map::{compute_cell_pair, CellPair};
use benilla_world_runtime::native::{
    AttachedInitialCreatureCell, FreshCreatureCellObject, Profile,
};
use std::{
    cell::{OnceCell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

/// Explicit effective host inputs; no server configuration or default is inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridConfiguration {
    pub expiry_ms: u32,
    pub unload_enabled: bool,
    pub vmap: VmapLoadConfiguration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GridStage {
    Allocated,
    Stored,
    Linked,
    Idle,
    TerrainLoaded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GridState {
    Invalid,
    Idle,
}
type GridKey = (u16, u16);
#[cfg(test)]
mod initial_creatures;

// Uninhabited categories still represent only fresh empty containers. Ordinary
// creatures additionally support the isolated constructor-stage attachment below;
// neither representation grants live registration or roster-absence evidence.
enum WorldPlayer {}
enum WorldPet {}
enum WorldCorpse {}
enum WorldCamera {}
enum GridGameObject {}
// Membership is installed before CurrentCell assignment. No pending entry can
// escape a failed consumed-grid construction transaction.
struct GridCreature {
    fresh: RefCell<Option<FreshCreatureCellObject>>,
    attached: OnceCell<AttachedInitialCreatureCell>,
}
enum GridDynamicObject {}
enum GridBones {}
enum ActiveGridObject {}
#[derive(Default)]
struct CellStores {
    players: Vec<WorldPlayer>,
    pets: Vec<WorldPet>,
    corpses: Vec<WorldCorpse>,
    cameras: Vec<WorldCamera>,
    game_objects: Vec<GridGameObject>,
    creatures: Vec<Rc<GridCreature>>,
    dynamic_objects: Vec<GridDynamicObject>,
    bones: Vec<GridBones>,
    active: Vec<ActiveGridObject>,
}
impl CellStores {
    fn empty(&self) -> bool {
        self.players.is_empty()
            && self.pets.is_empty()
            && self.corpses.is_empty()
            && self.cameras.is_empty()
            && self.game_objects.is_empty()
            && self.creatures.is_empty()
            && self.dynamic_objects.is_empty()
            && self.bones.is_empty()
            && self.active.is_empty()
    }
}
struct GridNode<'terrain, 'bundle, 'source> {
    key: GridKey,
    id: u32,
    state: GridState,
    cells: Box<[[CellStores; 16]; 16]>,
    timer_ms: u32,
    active_locks: u16,
    explicit_lock: bool,
    object_data_loaded: bool,
    terrain: Option<LoadedTerrain<'terrain, 'bundle, 'source>>,
}
struct LoadedTerrain<'terrain, 'bundle, 'source> {
    height: source_height::PreparedSourceHeight<'terrain, 'bundle, 'source>,
    query: Option<SourceMapQuerySnapshot>,
    retained: RetainedSourceQuery,
    references: u16,
    attempts: u32,
    tile: TileId,
    configuration: VmapLoadConfiguration,
}
/// Actual owned slot and insertion-ordered map-link collection. Typed cells
/// retain only construction-stage state, never source-world absence evidence.
/// This map-grid substate does not impersonate the full source Map lifecycle.
pub(crate) struct FreshSingleGrid<'terrain, 'bundle, 'source> {
    slot: SourceMapQuerySlot,
    profile_identity: benilla_world_runtime::Digest,
    configuration: GridConfiguration,
    nodes: BTreeMap<GridKey, Box<GridNode<'terrain, 'bundle, 'source>>>,
    linked: Vec<GridKey>,
    initial_guids: BTreeMap<u64, Rc<GridCreature>>,
    initial_dbguids: BTreeMap<(u16, u32), Vec<Rc<GridCreature>>>,
}
impl<'terrain, 'bundle, 'source> FreshSingleGrid<'terrain, 'bundle, 'source> {
    pub(crate) fn from_generated(
        input: &PreparedNavigationInput<'terrain, 'bundle, 'source>,
        generated: &GeneratedNavigation,
        profile: &Profile,
        position: [u32; 2],
        configuration: Option<GridConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, GenerationError> {
        Self::from_generated_observed(
            input,
            generated,
            profile,
            position,
            configuration,
            deadline,
            cancelled,
            |_| {},
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit creation operands and stage instrumentation"
    )]
    pub(crate) fn from_generated_observed(
        input: &PreparedNavigationInput<'terrain, 'bundle, 'source>,
        generated: &GeneratedNavigation,
        profile: &Profile,
        position: [u32; 2],
        configuration: Option<GridConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observed: impl FnMut(GridStage),
    ) -> Result<Self, GenerationError> {
        poll(deadline, &cancelled)?;
        let configuration = configuration.ok_or(Error::Missing)?;
        let cell = position_cell(position)?;
        let key = (cell.grid_x(), cell.grid_y());
        if input.tile
            != (TileId {
                x: (63 - key.0) as u8,
                y: (63 - key.1) as u8,
            })
        {
            return Err(Error::Identity.into());
        }
        let selected = profile.pathfinder_configuration().ok_or(Error::Missing)?;
        let slot = SourceMapQuerySlot {
            map_id: profile.map_id,
            instance_id: selected.selected().source_instance_id,
        };
        let node = Box::new(GridNode {
            key,
            id: u32::from(key.0) * 64 + u32::from(key.1),
            state: GridState::Invalid,
            cells: Box::new(std::array::from_fn(|_| {
                std::array::from_fn(|_| CellStores::default())
            })),
            timer_ms: configuration.expiry_ms,
            active_locks: 0,
            explicit_lock: !configuration.unload_enabled,
            object_data_loaded: false,
            terrain: None,
        });
        observed(GridStage::Allocated);
        poll(deadline, &cancelled)?;
        let mut owner = Self {
            slot,
            profile_identity: profile.digest(),
            configuration,
            nodes: BTreeMap::new(),
            linked: Vec::new(),
            initial_guids: BTreeMap::new(),
            initial_dbguids: BTreeMap::new(),
        };
        owner.nodes.insert(key, node);
        observed(GridStage::Stored);
        poll(deadline, &cancelled)?;
        // Source GridReference::targetObjectBuildLink inserts first and counts it.
        // The key resolves to the very same owned node, not a linked flag.
        owner.linked.insert(0, key);
        observed(GridStage::Linked);
        poll(deadline, &cancelled)?;
        owner.nodes.get_mut(&key).ok_or(Error::Identity)?.state = GridState::Idle;
        observed(GridStage::Idle);
        poll(deadline, &cancelled)?;
        let load = PositiveTileLoad::from_generated(
            input,
            generated,
            profile,
            Some(configuration.vmap),
            deadline,
            &cancelled,
        )?;
        if load.query().query().slot() != slot {
            return Err(Error::Identity.into());
        }
        let retained = load.query().query().retain_default();
        let references = load.references();
        let attempts = load.load_attempts();
        let tile = load.tile();
        let load_configuration = load.configuration();
        let (height, query) = load.into_parts();
        retained
            .check_query(query.query())
            .map_err(GenerationError::Native)?;
        owner.nodes.get_mut(&key).ok_or(Error::Identity)?.terrain = Some(LoadedTerrain {
            height,
            query: Some(query),
            retained,
            references,
            attempts,
            tile,
            configuration: load_configuration,
        });
        observed(GridStage::TerrainLoaded);
        owner.ensure(position, deadline, &cancelled)?;
        Ok(owner)
    }
    /// Existing NGrid returns without loading or taking a further TerrainInfo ref.
    /// Host freshness refusal is not a source retry or partial-failure cache.
    pub(crate) fn ensure(
        &self,
        position: [u32; 2],
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<CellPair, GenerationError> {
        poll(deadline, &cancelled)?;
        let cell = position_cell(position)?;
        let key = (cell.grid_x(), cell.grid_y());
        if self.linked.as_slice() != [key] || self.nodes.len() != 1 {
            return Err(Error::Identity.into());
        }
        let node = self.nodes.get(&key).ok_or(Error::Missing)?;
        if node.key != key || node.state != GridState::Idle {
            return Err(Error::Identity.into());
        }
        let terrain = node.terrain.as_ref().ok_or(Error::Missing)?;
        terrain
            .height
            .verify_unchanged(|| cancelled() || Instant::now() >= deadline)
            .map_err(GenerationError::Composition)?;
        poll(deadline, &cancelled)?;
        Ok(cell)
    }
    fn node(&self) -> &GridNode<'terrain, 'bundle, 'source> {
        &self.nodes[&self.linked[0]]
    }
    fn terrain(&self) -> &LoadedTerrain<'terrain, 'bundle, 'source> {
        self.node()
            .terrain
            .as_ref()
            .expect("only complete positive creation is published")
    }
    pub(crate) fn height(
        &self,
    ) -> &source_height::PreparedSourceHeight<'terrain, 'bundle, 'source> {
        &self.terrain().height
    }
    pub(crate) fn check_query(
        &self,
        query: &SourceMapQuerySnapshot,
    ) -> Result<(), GenerationError> {
        self.terrain()
            .retained
            .check_query(query.query())
            .map_err(GenerationError::Native)
    }
    pub(crate) fn grid_id(&self) -> u32 {
        self.node().id
    }
    pub(crate) fn configuration(&self) -> GridConfiguration {
        self.configuration
    }
    pub(crate) fn slot(&self) -> SourceMapQuerySlot {
        self.slot
    }
    pub(crate) fn linked_count(&self) -> usize {
        self.linked.len()
    }
    pub(crate) fn cell_count(&self) -> usize {
        self.node().cells.iter().map(|row| row.len()).sum()
    }
    pub(crate) fn cells_empty(&self) -> bool {
        self.node().cells.iter().flatten().all(CellStores::empty)
    }
    pub(crate) fn object_data_loaded(&self) -> bool {
        self.node().object_data_loaded
    }
    pub(crate) fn timer_ms(&self) -> u32 {
        self.node().timer_ms
    }
    pub(crate) fn active_locks(&self) -> u16 {
        self.node().active_locks
    }
    pub(crate) fn explicit_lock(&self) -> bool {
        self.node().explicit_lock
    }
    pub(crate) fn references(&self) -> u16 {
        self.terrain().references
    }
    pub(crate) fn load_attempts(&self) -> u32 {
        self.terrain().attempts
    }
    pub(crate) fn tile(&self) -> TileId {
        self.terrain().tile
    }
    pub(crate) fn vmap_configuration(&self) -> VmapLoadConfiguration {
        self.terrain().configuration
    }
    /// Keep actual node/link, height and Rc query allocation alive throughout the
    /// consumer callback. A value returned from the callback carries no grid
    /// certificate. Dropping this host scope is not source unload/cleanup.
    pub(crate) fn with_query<R>(
        mut self,
        consume: impl FnOnce(&Self, SourceMapQuerySnapshot) -> R,
    ) -> Result<R, GenerationError> {
        let key = self.linked[0];
        let query = self
            .nodes
            .get_mut(&key)
            .ok_or(Error::Missing)?
            .terrain
            .as_mut()
            .ok_or(Error::Missing)?
            .query
            .take()
            .ok_or(Error::Missing)?;
        self.check_query(&query)?;
        Ok(consume(&self, query))
    }
}
fn position_cell(position: [u32; 2]) -> Result<CellPair, GenerationError> {
    compute_cell_pair(position[0], position[1]).map_err(|_| Error::Unsupported.into())
}
fn poll(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), GenerationError> {
    if cancelled() {
        return Err(GenerationError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(GenerationError::Deadline);
    }
    Ok(())
}
