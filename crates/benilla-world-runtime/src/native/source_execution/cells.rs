// SPDX-License-Identifier: GPL-3.0-or-later
//! Grid lifetime and intrusive source type lists, not a pose-derived index.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::{BTreeMap, BTreeSet};
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
    update_marks: Box<[u64]>,
    grid_active: BTreeMap<(u16, u16), BTreeSet<u64>>,
    pub(super) removals: SourceRemovals,
}
impl SourceCells {
    pub fn new(map: MapIdentity) -> Result<Self> {
        Ok(Self {
            grids: SourceGrids::new(map)?,
            active: SourceActive::default(),
            cells: BTreeMap::new(),
            links: BTreeMap::new(),
            update_marks: vec![0; 1024 * 1024 / 64].into_boxed_slice(),
            grid_active: BTreeMap::new(),
            removals: SourceRemovals::default(),
        })
    }
    pub fn map(&self) -> MapIdentity {
        self.grids.map()
    }
    pub(super) fn reset_update_marks(&mut self) {
        self.update_marks.fill(0);
    }
    pub(super) fn mark_update_cell(&mut self, pair: CellPair) -> bool {
        let id = usize::from(pair.y()) * 1024 + usize::from(pair.x());
        let bit = 1_u64 << (id % 64);
        let word = &mut self.update_marks[id / 64];
        let fresh = *word & bit == 0;
        *word |= bit;
        fresh
    }
    // Grid::Add/RemoveGridObject capture the flag at those calls, not at query time; Grid.h:93-117.
    pub(super) fn grid_add(&mut self, object: &SourceObject, cell: CellIdentity) -> Result<()> {
        self.validate_cell(cell)?;
        if object.active && object.kind.cell_slot().is_some_and(|slot| slot < 4) {
            let pointer = object
                .source_allocation
                .ok_or_else(|| refuse("execution.residency.grid_activity_allocation"))?;
            self.grid_active
                .entry((cell.x, cell.y))
                .or_default()
                .insert(pointer);
        }
        Ok(())
    }
    pub(super) fn grid_remove(&mut self, object: &SourceObject) -> Result<()> {
        if !object.active || object.kind.cell_slot().is_none_or(|slot| slot >= 4) {
            return Ok(());
        }
        let Some(cell) = self.registered_cell(object.handle) else {
            return Ok(());
        };
        let pointer = object
            .source_allocation
            .ok_or_else(|| refuse("execution.residency.grid_activity_allocation"))?;
        if let Some(members) = self.grid_active.get_mut(&(cell.x, cell.y)) {
            members.remove(&pointer);
        }
        Ok(())
    }
    pub fn active_objects_in_grid(&self, grid: GridIdentity) -> Result<u32> {
        self.validate_grid(grid)?;
        let mut count = 0_u32;
        for x in grid.x * 16..grid.x * 16 + 16 {
            for y in grid.y * 16..grid.y * 16 + 16 {
                count = count.wrapping_add(
                    self.cells
                        .get(&(x, y))
                        .map_or(0, |slots| slots[4].count as u32),
                );
                count = count.wrapping_add(
                    self.grid_active
                        .get(&(x, y))
                        .map_or(0, |members| members.len() as u32),
                );
            }
        }
        Ok(count)
    }
    pub(super) fn first_in_slot(
        &self,
        cell: CellIdentity,
        slot: usize,
    ) -> Result<Option<ObjectHandle>> {
        self.validate_cell(cell)?;
        require(slot < 8, "execution.residency.cell_slot")?;
        Ok(self
            .cells
            .get(&(cell.x, cell.y))
            .and_then(|slots| slots[slot].first))
    }
    pub(super) fn next_in_slot(
        &self,
        object: ObjectHandle,
        cell: CellIdentity,
        slot: usize,
    ) -> Result<Option<ObjectHandle>> {
        self.validate_cell(cell)?;
        let link = self
            .links
            .get(&object)
            .ok_or_else(|| refuse("execution.residency.invalidated_cell_iterator"))?;
        require(
            link.cell == cell && link.slot == slot,
            "execution.residency.changed_cell_iterator",
        )?;
        Ok(link.next)
    }
    pub(super) fn destroy_unloaded_grid(&mut self, grid: GridIdentity) -> Result<()> {
        self.validate_grid(grid)?;
        require(
            !self
                .links
                .values()
                .any(|link| link.cell.grid == grid && link.slot < 4),
            "execution.residency.grid_objects_remain",
        )?;
        // Reverse member/array destruction invalidates world references; NGrid.h:195, TypeContainer.h:202-203, RefManager.h:35,49-55.
        for x in (grid.x * 16..grid.x * 16 + 16).rev() {
            for y in (grid.y * 16..grid.y * 16 + 16).rev() {
                let cell = self.cell(
                    CellPair::from_indices(x, y)
                        .map_err(|_| refuse("execution.cell_coordinates"))?,
                )?;
                for slot in (4..8).rev() {
                    while let Some(object) = self.first_in_slot(cell, slot)? {
                        self.unlink(object);
                    }
                }
            }
        }
        self.destroy_empty_grid(grid)
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
    pub(super) fn validate_cell(&self, cell: CellIdentity) -> Result<()> {
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
    // The unload owner must drain live registrations before deleting the grid; Map.cpp:1299-1313.
    pub fn destroy_empty_grid(&mut self, grid: GridIdentity) -> Result<()> {
        self.validate_grid(grid)?;
        require(
            !self.links.values().any(|link| link.cell.grid == grid),
            "execution.grid_has_registrations",
        )?;
        self.cells
            .retain(|(x, y), _| (*x / 16, *y / 16) != (grid.x, grid.y));
        self.grid_active
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
