// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordered source load attempts over independently registered catalog definitions.
use super::super::production_compiler::VerifiedCatalog;
use super::*;
use benilla_sim::resident_map::{source_cell_visit, CellPair};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum GridSpawnKind {
    GameObject,
    Creature,
}
impl GridSpawnKind {
    fn table(self) -> &'static str {
        match self {
            Self::GameObject => "gameobject",
            Self::Creature => "creature",
        }
    }
    fn object_kind(self) -> SourceObjectKind {
        match self {
            Self::GameObject => SourceObjectKind::GameObject,
            Self::Creature => SourceObjectKind::Creature,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum GridSpawnOrigin {
    Static,
    Persistent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridLoadAttempt {
    pub grid: GridIdentity,
    pub cell: CellPair,
    pub kind: GridSpawnKind,
    pub origin: GridSpawnOrigin,
    pub guid: u32,
    pub catalog_ordinal: u64,
}
type SpawnSet = BTreeMap<u32, u64>;
pub struct GridSpawnSets {
    map: MapIdentity,
    sets: BTreeMap<(u16, u16, GridSpawnKind, GridSpawnOrigin), SpawnSet>,
}
impl GridSpawnSets {
    pub fn new(map: MapIdentity) -> Self {
        Self {
            map,
            sets: BTreeMap::new(),
        }
    }
    // Distinct static/persistent uint32 sets deduplicate only within each lane; ObjectGridLoader.cpp:199-218, MapPersistentStateMgr.h:43-51.
    pub fn register(
        &mut self,
        cell: CellPair,
        kind: GridSpawnKind,
        origin: GridSpawnOrigin,
        guid: u32,
        ordinal: u64,
    ) -> Result<()> {
        require(cell.validate(), "execution.cell_coordinates")?;
        let set = self
            .sets
            .entry((cell.x(), cell.y(), kind, origin))
            .or_default();
        if let Some(previous) = set.get(&guid) {
            require(*previous == ordinal, "execution.grid_definition_binding")?;
        }
        set.insert(guid, ordinal);
        Ok(())
    }
    pub fn remove(
        &mut self,
        cell: CellPair,
        kind: GridSpawnKind,
        origin: GridSpawnOrigin,
        guid: u32,
    ) {
        if let Some(set) = self.sets.get_mut(&(cell.x(), cell.y(), kind, origin)) {
            set.remove(&guid);
        }
    }
    fn next(
        &self,
        cell: CellPair,
        kind: GridSpawnKind,
        origin: GridSpawnOrigin,
        after: Option<u32>,
    ) -> Option<(u32, u64)> {
        let set = self.sets.get(&(cell.x(), cell.y(), kind, origin))?;
        let row = if let Some(after) = after {
            set.range((Excluded(after), Unbounded)).next()
        } else {
            set.first_key_value()
        };
        row.map(|(&guid, &ordinal)| (guid, ordinal))
    }
}
pub trait GridDefinitions {
    type Row;
    fn read(&mut self, attempt: GridLoadAttempt) -> Result<Self::Row>;
}
pub struct CatalogGridDefinitions<'a> {
    catalog: &'a mut VerifiedCatalog,
}
impl<'a> CatalogGridDefinitions<'a> {
    pub fn new(catalog: &'a mut VerifiedCatalog) -> Self {
        Self { catalog }
    }
}
impl GridDefinitions for CatalogGridDefinitions<'_> {
    type Row = super::super::production_compiler::RetainedCatalogRow;
    fn read(&mut self, attempt: GridLoadAttempt) -> Result<Self::Row> {
        Ok(self.catalog.execution_spawn_row(
            attempt.kind.table(),
            attempt.guid,
            attempt.catalog_ordinal,
        )?)
    }
}
pub trait GridProductOwner {
    fn load(&mut self, map: MapIdentity, coordinates: ProductCoordinates) -> Result<bool>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridCandidateOutcome {
    Loaded(ObjectHandle),
    FailedAndDestroyed(ObjectHandle),
}
pub struct GridFactoryContext<'a, D, P> {
    pub definitions: &'a mut D,
    pub products: &'a mut P,
    pub sets: &'a mut GridSpawnSets,
    pub objects: &'a mut SourceObjects,
    pub cells: &'a mut SourceCells,
}
impl<D: GridDefinitions, P: GridProductOwner> GridFactoryContext<'_, D, P> {
    // Active construction and viewpoint callbacks may recursively load through the same owners; Map.cpp:392-399, Camera.cpp:147-151.
    pub fn load_grid<F: GridObjectFactory<D, P>>(
        &mut self,
        entry: CellPair,
        trigger: GridLoadTrigger,
        factory: &mut F,
    ) -> Result<Option<GridIdentity>> {
        let mut loader = SourceGridLoader {
            definitions: &mut *self.definitions,
            factory,
            products: &mut *self.products,
            sets: &mut *self.sets,
            objects: &mut *self.objects,
        };
        self.cells.trigger_grid_load(entry, trigger, &mut loader)
    }
}
// Factories own real constructors, LoadFromDB, failure deletion and reached callbacks; ObjectGridLoader.cpp:119-159.
pub trait GridObjectFactory<D: GridDefinitions, P: GridProductOwner>: Sized {
    fn load_from_db(
        &mut self,
        attempt: GridLoadAttempt,
        row: D::Row,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<GridCandidateOutcome>;
    fn viewpoint_added(
        &mut self,
        object: ObjectHandle,
        cell: CellIdentity,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn battleground_db_load(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn cell_corpses(
        &mut self,
        cell: CellPair,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn resurrectable_corpses(
        &mut self,
        entry: CellPair,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
}
pub struct SourceGridLoader<'a, D, F, P> {
    pub definitions: &'a mut D,
    pub factory: &'a mut F,
    pub products: &'a mut P,
    pub sets: &'a mut GridSpawnSets,
    pub objects: &'a mut SourceObjects,
}
impl<D: GridDefinitions, F: GridObjectFactory<D, P>, P: GridProductOwner> GridLoadOwner
    for SourceGridLoader<'_, D, F, P>
{
    fn load_products(&mut self, map: MapIdentity, coordinates: ProductCoordinates) -> Result<bool> {
        self.products.load(map, coordinates)
    }
    // LoadN walks x outside y, grid types before world types, and static before persistent; ObjectGridLoader.cpp:192-264, GridDefines.h:68-69.
    fn load_objects(&mut self, cells: &mut SourceCells, grid: GridIdentity) -> Result<()> {
        require(
            self.sets.map == grid.map && cells.map() == grid.map,
            "execution.grid_loader_map",
        )?;
        cells.grids().validate(grid)?;
        let mut context = GridFactoryContext {
            definitions: &mut *self.definitions,
            products: &mut *self.products,
            sets: &mut *self.sets,
            objects: &mut *self.objects,
            cells,
        };
        for x in 0..16 {
            for y in 0..16 {
                let cell = CellPair::from_indices(grid.x * 16 + x, grid.y * 16 + y)
                    .map_err(|_| refuse("execution.cell_coordinates"))?;
                for kind in [GridSpawnKind::GameObject, GridSpawnKind::Creature] {
                    for origin in [GridSpawnOrigin::Static, GridSpawnOrigin::Persistent] {
                        let mut after = None;
                        while let Some((guid, catalog_ordinal)) =
                            context.sets.next(cell, kind, origin, after)
                        {
                            after = Some(guid);
                            let attempt = GridLoadAttempt {
                                grid,
                                cell,
                                kind,
                                origin,
                                guid,
                                catalog_ordinal,
                            };
                            let row = context.definitions.read(attempt)?;
                            match self.factory.load_from_db(attempt, row, &mut context)? {
                                GridCandidateOutcome::FailedAndDestroyed(handle) => require(
                                    context.objects.get(handle).is_err(),
                                    "execution.grid_failed_candidate_alive",
                                )?,
                                GridCandidateOutcome::Loaded(handle) => {
                                    let object = context.objects.get(handle)?;
                                    let kind_matches = object.kind == kind.object_kind()
                                        || (kind == GridSpawnKind::GameObject
                                            && object.kind == SourceObjectKind::Transport);
                                    require(
                                        kind_matches
                                            && object.in_world
                                            && object.map == Some(grid.map),
                                        "execution.grid_candidate_not_in_world",
                                    )?;
                                    let loader_cell = context.cells.cell(cell)?;
                                    if kind == GridSpawnKind::GameObject {
                                        context.objects.link_cell(
                                            handle,
                                            loader_cell,
                                            context.cells,
                                        )?;
                                    }
                                    require(
                                        context.cells.registered_cell(handle).is_some(),
                                        "execution.grid_candidate_not_registered",
                                    )?;
                                    self.factory.viewpoint_added(
                                        handle,
                                        loader_cell,
                                        &mut context,
                                    )?;
                                    self.factory.battleground_db_load(handle, &mut context)?;
                                }
                            }
                        }
                    }
                }
                self.factory.cell_corpses(cell, &mut context)?;
            }
        }
        Ok(())
    }
    fn add_resurrectable_corpses(
        &mut self,
        cells: &mut SourceCells,
        _grid: GridIdentity,
        entry: CellPair,
    ) -> Result<()> {
        self.factory.resurrectable_corpses(
            entry,
            &mut GridFactoryContext {
                definitions: &mut *self.definitions,
                products: &mut *self.products,
                sets: &mut *self.sets,
                objects: &mut *self.objects,
                cells,
            },
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CameraCellVisit {
    pub standing: CellPair,
    pub x_bits: u32,
    pub y_bits: u32,
    pub radius_bits: u32,
    pub bounding_radius_bits: u32,
    pub no_create: bool,
}
impl SourceCells {
    // Camera VisitAllObjects performs complete grid then world passes, preserving NoCreate; Camera.cpp:147-151, CellImpl.h:197-209.
    pub fn visit_camera<O: GridLoadOwner>(
        &mut self,
        request: CameraCellVisit,
        owner: &mut O,
        mut visit: impl FnMut(&mut Self, &mut O, CellIdentity, CellContainer) -> Result<()>,
    ) -> Result<()> {
        for container in [CellContainer::Grid, CellContainer::World] {
            let pairs = source_cell_visit(
                request.standing,
                request.x_bits,
                request.y_bits,
                request.radius_bits,
                request.bounding_radius_bits,
            )
            .map_err(|_| refuse("execution.cell_visit_numeric"))?;
            for pair in pairs {
                if self
                    .trigger_grid_load(
                        pair,
                        GridLoadTrigger::Visit {
                            no_create: request.no_create,
                        },
                        owner,
                    )?
                    .is_some()
                {
                    let cell = self.cell(pair)?;
                    visit(self, owner, cell, container)?;
                }
            }
        }
        Ok(())
    }
}

impl CameraCellVisit {
    // Camera entry uses the maximum source distance and permits loading; Camera.cpp:147-151.
    pub fn visibility(
        x_bits: u32,
        y_bits: u32,
        bounding_radius_bits: u32,
        visibility_bits: u32,
        add_to_world: bool,
    ) -> Result<Self> {
        let standing = benilla_sim::resident_map::compute_cell_pair(x_bits, y_bits)
            .map_err(|_| refuse("execution.cell_coordinates"))?;
        Ok(Self {
            standing,
            x_bits,
            y_bits,
            bounding_radius_bits,
            radius_bits: if add_to_world {
                0x4405_5555
            } else {
                visibility_bits
            },
            no_create: false,
        })
    }
}
impl SourceObjects {
    // Player entry always registers its world-list cell, even when already loaded; Map.cpp:357-381.
    pub fn enter_player_grid(
        &mut self,
        player: ObjectHandle,
        entry: CellPair,
        cells: &mut SourceCells,
        owner: &mut impl GridLoadOwner,
    ) -> Result<()> {
        require(
            self.get(player)?.kind == SourceObjectKind::Player,
            "execution.grid_entry_not_player",
        )?;
        cells.trigger_grid_load(entry, GridLoadTrigger::Enter, owner)?;
        self.link_cell(player, cells.cell(entry)?, cells)
    }
}
