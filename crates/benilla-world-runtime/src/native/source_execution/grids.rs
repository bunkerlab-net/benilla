// SPDX-License-Identifier: GPL-3.0-or-later
//! Source NGrid linkage and load ownership, independent of immutable definitions.
use super::*;
use benilla_sim::resident_map::CellPair;
use benilla_sim::source_time::TimeTracker;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum NGridPhase {
    Invalid,
    Active,
    Idle,
    Removal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ProductCoordinates {
    pub x: u16,
    pub y: u16,
}
impl GridIdentity {
    // Convert internal grids to terrain/vmap coordinates; Map.cpp:348-353.
    pub fn product_coordinates(self) -> ProductCoordinates {
        ProductCoordinates {
            x: 63 - self.x,
            y: 63 - self.y,
        }
    }
    pub fn source_grid_id(self) -> u32 {
        u32::from(self.x) * 64 + u32::from(self.y)
    }
}
struct NGrid {
    identity: GridIdentity,
    previous: Option<(u16, u16)>,
    next: Option<(u16, u16)>,
    phase: NGridPhase,
    products: Option<bool>,
    object_data_loaded: bool,
    blocked: Option<Refusal>,
    explicit_lock: Option<bool>,
    active_lock_count: u16,
    expiry: Option<TimeTracker>,
}
pub struct SourceGrids {
    map: MapIdentity,
    next_generation: u64,
    first: Option<(u16, u16)>,
    nodes: BTreeMap<(u16, u16), NGrid>,
    configuration: Option<(u32, bool)>,
}
impl SourceGrids {
    pub fn new(map: MapIdentity) -> Result<Self> {
        require(map.creation_generation > 0, "execution.cell_map_generation")?;
        Ok(Self {
            map,
            next_generation: 1,
            first: None,
            nodes: BTreeMap::new(),
            configuration: None,
        })
    }
    pub fn map(&self) -> MapIdentity {
        self.map
    }
    // Bind source GridInfo construction inputs before any grid exists; NGrid.h:40-42,89-93.
    pub fn configure(&mut self, expiry: u32, unload: bool) -> Result<()> {
        require(
            self.nodes.is_empty() && self.configuration.is_none(),
            "execution.grid_configuration",
        )?;
        self.configuration = Some((expiry, unload));
        Ok(())
    }
    pub fn identity(&self, x: u16, y: u16) -> Option<GridIdentity> {
        self.nodes.get(&(x, y)).map(|n| n.identity)
    }
    fn node(&self, grid: GridIdentity) -> Result<&NGrid> {
        self.nodes
            .get(&(grid.x, grid.y))
            .filter(|n| n.identity == grid)
            .ok_or_else(|| refuse("execution.stale_grid"))
    }
    fn node_mut(&mut self, grid: GridIdentity) -> Result<&mut NGrid> {
        self.node(grid)?;
        Ok(self.nodes.get_mut(&(grid.x, grid.y)).unwrap())
    }
    // Insert new NGrids at the reference-list front, then set IDLE; Map.cpp:340-346, GridReference.h:31-35.
    pub fn create(&mut self, x: u16, y: u16) -> Result<GridIdentity> {
        require(x < 64 && y < 64, "execution.grid_coordinates")?;
        if let Some(identity) = self.identity(x, y) {
            return Ok(identity);
        }
        let generation = self.next_generation;
        self.next_generation = generation
            .checked_add(1)
            .ok_or_else(|| refuse("execution.grid_generation_exhausted"))?;
        let identity = GridIdentity {
            map: self.map,
            x,
            y,
            generation,
        };
        if let Some(first) = self.first {
            self.nodes.get_mut(&first).unwrap().previous = Some((x, y));
        }
        self.nodes.insert(
            (x, y),
            NGrid {
                identity,
                previous: None,
                next: self.first,
                phase: NGridPhase::Idle,
                products: None,
                object_data_loaded: false,
                blocked: None,
                explicit_lock: self.configuration.map(|(_, unload)| !unload),
                active_lock_count: 0,
                expiry: self
                    .configuration
                    .map(|(expiry, _)| TimeTracker::new(i64::from(expiry))),
            },
        );
        self.first = Some((x, y));
        Ok(identity)
    }
    pub fn linked(&self) -> impl Iterator<Item = GridIdentity> + '_ {
        let mut next = self.first;
        std::iter::from_fn(move || {
            let n = self.nodes.get(&next?)?;
            next = n.next;
            Some(n.identity)
        })
    }
    pub(super) fn next_linked(&self, grid: GridIdentity) -> Result<Option<GridIdentity>> {
        Ok(self
            .node(grid)?
            .next
            .and_then(|key| self.nodes.get(&key).map(|node| node.identity)))
    }
    pub fn phase(&self, grid: GridIdentity) -> Result<NGridPhase> {
        Ok(self.node(grid)?.phase)
    }
    pub fn product_loaded(&self, grid: GridIdentity) -> Result<bool> {
        Ok(self.node(grid)?.products == Some(true))
    }
    pub fn products_attempted(&self, grid: GridIdentity) -> Result<bool> {
        Ok(self.node(grid)?.products.is_some())
    }
    pub fn object_data_loaded(&self, x: u16, y: u16) -> bool {
        self.nodes
            .get(&(x, y))
            .is_some_and(|n| n.object_data_loaded)
    }
    pub fn explicit_lock(&self, grid: GridIdentity) -> Result<bool> {
        self.node(grid)?
            .explicit_lock
            .ok_or_else(|| refuse("execution.pending_grid_configuration"))
    }
    pub fn active_lock_count(&self, grid: GridIdentity) -> Result<u16> {
        Ok(self.node(grid)?.active_lock_count)
    }
    pub fn unload_locked(&self, grid: GridIdentity) -> Result<bool> {
        Ok(self.active_lock_count(grid)? != 0 || self.explicit_lock(grid)?)
    }
    // Map::SetUnloadLock binds the independent explicit bit; Map.h:172.
    pub fn set_explicit_lock(&mut self, grid: GridIdentity, on: bool) -> Result<()> {
        self.explicit_lock(grid)?;
        self.node_mut(grid)?.explicit_lock = Some(on);
        Ok(())
    }
    // The uint16 bitfield wraps on increment and guards zero on decrement; NGrid.h:53-64.
    pub(super) fn change_active_lock(&mut self, grid: GridIdentity, add: bool) -> Result<u16> {
        let count = &mut self.node_mut(grid)?.active_lock_count;
        *count = if add {
            count.wrapping_add(1)
        } else {
            count.saturating_sub(1)
        };
        Ok(*count)
    }
    pub fn expiry(&self, grid: GridIdentity) -> Result<i64> {
        self.node(grid)?
            .expiry
            .map(|timer| timer.get_expiry())
            .ok_or_else(|| refuse("execution.pending_grid_configuration"))
    }
    pub(super) fn set_phase(&mut self, grid: GridIdentity, phase: NGridPhase) -> Result<()> {
        self.node_mut(grid)?.phase = phase;
        Ok(())
    }
    pub(super) fn advance_expiry(&mut self, grid: GridIdentity, diff: u32) -> Result<bool> {
        let timer = self
            .node_mut(grid)?
            .expiry
            .as_mut()
            .ok_or_else(|| refuse("execution.pending_grid_configuration"))?;
        timer
            .update(i64::from(diff))
            .map_err(|_| refuse("execution.residency.expiry_overflow"))?;
        Ok(timer.passed())
    }
    pub(super) fn reset_expiry(&mut self, grid: GridIdentity, short: bool) -> Result<()> {
        let (expiry, _) = self
            .configuration
            .ok_or_else(|| refuse("execution.pending_grid_configuration"))?;
        // Promote binary32 factor before multiplication, then truncate time_t; Map.h:177-179.
        let interval = if short {
            (f64::from(expiry) * f64::from(0.1_f32)) as i64
        } else {
            i64::from(expiry)
        };
        self.node_mut(grid)?
            .expiry
            .as_mut()
            .ok_or_else(|| refuse("execution.pending_grid_configuration"))?
            .reset(interval);
        Ok(())
    }
    pub(super) fn validate(&self, grid: GridIdentity) -> Result<()> {
        self.node(grid).map(|_| ())
    }
    pub(super) fn mark_objects(&mut self, grid: GridIdentity) -> Result<()> {
        self.node_mut(grid)?.object_data_loaded = true;
        Ok(())
    }
    pub(super) fn destroy(&mut self, grid: GridIdentity) -> Result<()> {
        let n = self.node(grid)?;
        let (previous, next) = (n.previous, n.next);
        if let Some(previous) = previous {
            self.nodes.get_mut(&previous).unwrap().next = next;
        } else {
            self.first = next;
        }
        if let Some(next) = next {
            self.nodes.get_mut(&next).unwrap().previous = previous;
        }
        self.nodes.remove(&(grid.x, grid.y));
        Ok(())
    }
}
// Reached dependencies execute through their actual owners, never expected trace outcomes; Map.cpp:151,398-402.
pub trait GridLoadOwner {
    fn load_products(&mut self, map: MapIdentity, coordinates: ProductCoordinates) -> Result<bool>;
    fn load_objects(&mut self, cells: &mut SourceCells, grid: GridIdentity) -> Result<()>;
    fn add_resurrectable_corpses(
        &mut self,
        cells: &mut SourceCells,
        grid: GridIdentity,
        entry: CellPair,
    ) -> Result<()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridLoadTrigger {
    TerrainOnly,
    Visit { no_create: bool },
    Enter,
    Force,
    ActiveObject,
}
impl SourceCells {
    pub fn grids(&self) -> &SourceGrids {
        &self.grids
    }
    pub fn configure_grids(&mut self, expiry: u32, unload: bool) -> Result<()> {
        self.grids.configure(expiry, unload)
    }
    pub fn ensure_created(
        &mut self,
        x: u16,
        y: u16,
        owner: &mut impl GridLoadOwner,
    ) -> Result<GridIdentity> {
        require(
            self.grids.configuration.is_some(),
            "execution.pending_grid_configuration",
        )?;
        let existed = self.grids.identity(x, y).is_some();
        let grid = self.ensure_grid(x, y)?;
        if let Some(error) = &self.grids.node(grid)?.blocked {
            return Err(error.clone());
        }
        if !existed {
            match owner.load_products(grid.map, grid.product_coordinates()) {
                Ok(present) => self.grids.node_mut(grid)?.products = Some(present),
                Err(error) => {
                    self.grids.node_mut(grid)?.blocked = Some(error.clone());
                    return Err(error);
                }
            }
        }
        require(
            self.grids.products_attempted(grid)?,
            "execution.pending_grid_products",
        )?;
        Ok(grid)
    }
    // Publish the object-data bit before factories can recursively load; Map.cpp:390-402.
    pub fn ensure_loaded(
        &mut self,
        entry: CellPair,
        owner: &mut impl GridLoadOwner,
    ) -> Result<bool> {
        require(entry.validate(), "execution.cell_coordinates")?;
        let grid = self.ensure_created(entry.grid_x(), entry.grid_y(), owner)?;
        if self.object_data_loaded(grid.x, grid.y) {
            return Ok(false);
        }
        self.set_object_data_loaded(grid)?;
        let result = owner
            .load_objects(self, grid)
            .and_then(|()| owner.add_resurrectable_corpses(self, grid, entry));
        if let Err(error) = result {
            self.grids.node_mut(grid)?.blocked = Some(error.clone());
            return Err(error);
        }
        Ok(true)
    }
    // Only newly object-loaded entry grids become ACTIVE and receive the 0.1f expiry factor; Map.cpp:361-375.
    pub fn trigger_grid_load(
        &mut self,
        entry: CellPair,
        trigger: GridLoadTrigger,
        owner: &mut impl GridLoadOwner,
    ) -> Result<Option<GridIdentity>> {
        require(entry.validate(), "execution.cell_coordinates")?;
        let (x, y) = (entry.grid_x(), entry.grid_y());
        if matches!(trigger, GridLoadTrigger::Visit { no_create: true })
            && !self.object_data_loaded(x, y)
        {
            return Ok(None);
        }
        if trigger == GridLoadTrigger::TerrainOnly {
            return self.ensure_created(x, y, owner).map(Some);
        }
        let was_loaded = self.object_data_loaded(x, y);
        let loaded = self.ensure_loaded(entry, owner)?;
        let grid = self.grids.identity(x, y).unwrap();
        if loaded && matches!(trigger, GridLoadTrigger::Enter | GridLoadTrigger::Force) {
            self.grids.set_phase(grid, NGridPhase::Active)?;
            self.grids.reset_expiry(grid, true)?;
        }
        // ForceLoadGrid sets its explicit lock only in the initially-cold branch; Map.cpp:419-427.
        if trigger == GridLoadTrigger::Force && !was_loaded {
            self.grids.node_mut(grid)?.explicit_lock = Some(true);
        }
        Ok(Some(grid))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GridPending {
    SealedRegistration,
    LoadTriggerInputs,
    GeometryProductOwner,
    NativeConstructionOwners,
    CorpseAndViewpointOwners,
    LinkedTraversalEvidence,
    FullCausalJoins,
    GridConfiguration,
    NativeGenerationJoins,
}
pub const GRID_PENDING: [GridPending; 9] = [
    GridPending::SealedRegistration,
    GridPending::LoadTriggerInputs,
    GridPending::GeometryProductOwner,
    GridPending::NativeConstructionOwners,
    GridPending::CorpseAndViewpointOwners,
    GridPending::LinkedTraversalEvidence,
    GridPending::FullCausalJoins,
    GridPending::GridConfiguration,
    GridPending::NativeGenerationJoins,
];
