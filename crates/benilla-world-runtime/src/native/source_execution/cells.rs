// SPDX-License-Identifier: GPL-3.0-or-later
//! Grid lifetime and intrusive source type lists, not a pose-derived index.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct GridIdentity {
    pub map: MapIdentity,
    pub x: u16,
    pub y: u16,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellIdentity {
    pub grid: GridIdentity,
    pub x: u16,
    pub y: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellContainer {
    Grid,
    World,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellRelocation {
    SameCell,
    RefusedUnloaded,
    Moved {
        old: CellIdentity,
        new: CellIdentity,
    },
}
#[derive(Clone, Copy, Default)]
struct Ends {
    first: Option<ObjectHandle>,
    last: Option<ObjectHandle>,
    count: usize,
}
#[derive(Clone, Copy)]
struct Link {
    cell: CellIdentity,
    slot: usize,
    previous: Option<ObjectHandle>,
    next: Option<ObjectHandle>,
}
pub struct SourceCells {
    pub(super) grids: SourceGrids,
    pub(super) active: SourceActive,
    cells: BTreeMap<(u16, u16), [Ends; 8]>,
    links: BTreeMap<ObjectHandle, Link>,
}
impl SourceCells {
    pub fn new(map: MapIdentity) -> Result<Self> {
        Ok(Self {
            grids: SourceGrids::new(map)?,
            active: SourceActive::default(),
            cells: BTreeMap::new(),
            links: BTreeMap::new(),
        })
    }
    pub fn map(&self) -> MapIdentity {
        self.grids.map()
    }
    // Grid generations change only on reconstruction, not unload timer changes; Map.cpp:336-366.
    pub fn ensure_grid(&mut self, x: u16, y: u16) -> Result<GridIdentity> {
        self.grids.create(x, y)
    }
    fn validate_grid(&self, grid: GridIdentity) -> Result<()> {
        self.grids.validate(grid)
    }
    pub fn set_object_data_loaded(&mut self, grid: GridIdentity) -> Result<()> {
        self.grids.mark_objects(grid)
    }
    pub fn object_data_loaded(&self, x: u16, y: u16) -> bool {
        self.grids.object_data_loaded(x, y)
    }
    pub fn cell(&self, pair: CellPair) -> Result<CellIdentity> {
        require(pair.validate(), "execution.cell_coordinates")?;
        let grid = self
            .grids
            .identity(pair.grid_x(), pair.grid_y())
            .ok_or_else(|| refuse("execution.grid_absent"))?;
        Ok(CellIdentity {
            grid,
            x: pair.x(),
            y: pair.y(),
        })
    }
    fn validate_cell(&self, cell: CellIdentity) -> Result<()> {
        self.validate_grid(cell.grid)?;
        require(
            cell.x < 1024
                && cell.y < 1024
                && cell.x / 16 == cell.grid.x
                && cell.y / 16 == cell.grid.y,
            "execution.cell_coordinates",
        )
    }
    // Source link() unlinks an existing reference before inserting at the front; Reference.h:55-65, GridReference.h:31-35.
    pub(super) fn link(
        &mut self,
        object: ObjectHandle,
        kind: SourceObjectKind,
        cell: CellIdentity,
    ) -> Result<()> {
        self.validate_cell(cell)?;
        let slot = kind
            .cell_slot()
            .ok_or_else(|| refuse("execution.object_not_cell_type"))?;
        self.unlink(object);
        let ends = &mut self.cells.entry((cell.x, cell.y)).or_default()[slot];
        let next = ends.first;
        if let Some(next) = next {
            self.links.get_mut(&next).unwrap().previous = Some(object);
        } else {
            ends.last = Some(object);
        }
        ends.first = Some(object);
        ends.count += 1;
        self.links.insert(
            object,
            Link {
                cell,
                slot,
                previous: None,
                next,
            },
        );
        Ok(())
    }
    // Unlink is idempotent and adjusts both adjacent references; TypeContainerFunctions.h:98-101.
    pub(super) fn unlink(&mut self, object: ObjectHandle) -> Option<CellIdentity> {
        let link = self.links.remove(&object)?;
        let ends = &mut self.cells.get_mut(&(link.cell.x, link.cell.y)).unwrap()[link.slot];
        if let Some(previous) = link.previous {
            self.links.get_mut(&previous).unwrap().next = link.next;
        } else {
            ends.first = link.next;
        }
        if let Some(next) = link.next {
            self.links.get_mut(&next).unwrap().previous = link.previous;
        } else {
            ends.last = link.previous;
        }
        ends.count -= 1;
        Some(link.cell)
    }
    pub fn registered_cell(&self, object: ObjectHandle) -> Option<CellIdentity> {
        self.links.get(&object).map(|link| link.cell)
    }
    pub fn visit(&self, cell: CellIdentity, container: CellContainer) -> Result<CellObjects<'_>> {
        self.validate_cell(cell)?;
        let slot = if container == CellContainer::Grid {
            0
        } else {
            4
        };
        Ok(CellObjects {
            registry: self,
            cell,
            slot,
            end: slot + 4,
            current: None,
            started: false,
        })
    }
    // Map::Visit NoCreate checks object data, not mere geometry/grid existence; Map.cpp:660-663.
    pub fn visit_no_create(
        &self,
        pair: CellPair,
        container: CellContainer,
    ) -> Result<Option<CellObjects<'_>>> {
        if !self.object_data_loaded(pair.grid_x(), pair.grid_y()) {
            return Ok(None);
        }
        Ok(Some(self.visit(self.cell(pair)?, container)?))
    }
    // Reached cross-grid movement must execute its actual loader before relinking; Map.cpp:1219-1241.
    pub(super) fn relocate_creature(
        &mut self,
        object: ObjectHandle,
        kind: SourceObjectKind,
        target: CellPair,
        active: bool,
        load_at_enter: impl FnOnce(&mut Self, CellPair) -> Result<()>,
    ) -> Result<CellRelocation> {
        require(
            matches!(kind, SourceObjectKind::Creature | SourceObjectKind::Pet),
            "execution.creature_relocation_kind",
        )?;
        let old = self
            .registered_cell(object)
            .ok_or_else(|| refuse("execution.creature_not_registered"))?;
        require(target.validate(), "execution.cell_coordinates")?;
        if (old.grid.x, old.grid.y) != (target.grid_x(), target.grid_y()) {
            if !active && !self.object_data_loaded(target.grid_x(), target.grid_y()) {
                return Ok(CellRelocation::RefusedUnloaded);
            }
            load_at_enter(self, target)?;
            require(
                self.object_data_loaded(target.grid_x(), target.grid_y()),
                "execution.relocation_loader_not_complete",
            )?;
        }
        let new = self.cell(target)?;
        if new == old {
            return Ok(CellRelocation::SameCell);
        }
        self.link(object, kind, new)?;
        Ok(CellRelocation::Moved { old, new })
    }
    // The unload owner must drain live registrations before deleting the grid; Map.cpp:1299-1313.
    pub fn destroy_empty_grid(&mut self, grid: GridIdentity) -> Result<()> {
        self.validate_grid(grid)?;
        require(
            !self.links.values().any(|link| link.cell.grid == grid),
            "execution.grid_has_registrations",
        )?;
        self.cells
            .retain(|(x, y), _| (*x / 16, *y / 16) != (grid.x, grid.y));
        self.grids.destroy(grid)?;
        Ok(())
    }
}
pub struct CellObjects<'a> {
    registry: &'a SourceCells,
    cell: CellIdentity,
    slot: usize,
    end: usize,
    current: Option<ObjectHandle>,
    started: bool,
}
impl Iterator for CellObjects<'_> {
    type Item = ObjectHandle;
    // Type-list head precedes tail; each intrusive list starts at getFirst(); TypeContainerVisitor.h:55-58, GridRefManager.h:33-44.
    fn next(&mut self) -> Option<Self::Item> {
        while self.slot < self.end {
            if !self.started {
                self.current = self
                    .registry
                    .cells
                    .get(&(self.cell.x, self.cell.y))
                    .and_then(|lists| lists[self.slot].first);
                self.started = true;
            }
            if let Some(current) = self.current {
                self.current = self.registry.links.get(&current).unwrap().next;
                return Some(current);
            }
            self.slot += 1;
            self.started = false;
        }
        None
    }
}
