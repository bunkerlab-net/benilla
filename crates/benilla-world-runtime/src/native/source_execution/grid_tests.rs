// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::resident_map::{compute_cell_pair, source_cell_visit, CellPair};
use std::collections::BTreeSet;
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn pair(x: u16, y: u16) -> CellPair {
    CellPair::from_indices(x, y).unwrap()
}
fn cells() -> SourceCells {
    let mut cells = SourceCells::new(map()).unwrap();
    cells.configure_grids(300001, true).unwrap();
    cells
}
#[derive(Default)]
struct Work {
    products: usize,
    loads: usize,
    corpses: usize,
    recurse: bool,
    fail: bool,
    coordinates: Vec<ProductCoordinates>,
}
impl GridLoadOwner for Work {
    fn load_products(
        &mut self,
        _map: MapIdentity,
        coordinates: ProductCoordinates,
    ) -> Result<bool> {
        self.products += 1;
        self.coordinates.push(coordinates);
        Ok(true)
    }
    fn load_objects(&mut self, cells: &mut SourceCells, grid: GridIdentity) -> Result<()> {
        self.loads += 1;
        assert!(cells.object_data_loaded(grid.x, grid.y));
        if self.recurse {
            assert!(!cells.ensure_loaded(pair(grid.x * 16, grid.y * 16), self)?);
        }
        if self.fail {
            return Err(Refusal {
                id: "authored.reached_owner_missing".into(),
            });
        }
        Ok(())
    }
    fn add_resurrectable_corpses(
        &mut self,
        _cells: &mut SourceCells,
        _grid: GridIdentity,
        _entry: CellPair,
    ) -> Result<()> {
        self.corpses += 1;
        Ok(())
    }
}
#[test]
fn created_products_and_objects_are_distinct_and_recursion_does_not_reload() {
    let mut cells = cells();
    let mut work = Work {
        recurse: true,
        ..Work::default()
    };
    let grid = cells
        .trigger_grid_load(pair(32, 48), GridLoadTrigger::TerrainOnly, &mut work)
        .unwrap()
        .unwrap();
    assert!(cells.grids().product_loaded(grid).unwrap());
    assert!(!cells.object_data_loaded(2, 3));
    assert_eq!(cells.grids().phase(grid).unwrap(), NGridPhase::Idle);
    assert_eq!(work.coordinates, [ProductCoordinates { x: 61, y: 60 }]);
    assert_eq!(
        cells
            .trigger_grid_load(
                pair(48, 48),
                GridLoadTrigger::Visit { no_create: true },
                &mut work
            )
            .unwrap(),
        None
    );
    assert!(cells.grids().identity(3, 3).is_none());
    cells
        .trigger_grid_load(pair(32, 48), GridLoadTrigger::Enter, &mut work)
        .unwrap();
    assert_eq!((work.products, work.loads, work.corpses), (1, 1, 1));
    assert_eq!(cells.grids().phase(grid).unwrap(), NGridPhase::Active);
    assert_eq!(cells.grids().expiry(grid).unwrap(), 30000);
    assert!(!cells.grids().explicit_lock(grid).unwrap());
    cells
        .trigger_grid_load(pair(32, 48), GridLoadTrigger::Force, &mut work)
        .unwrap();
    assert!(!cells.grids().explicit_lock(grid).unwrap());
    let forced = cells
        .trigger_grid_load(pair(48, 48), GridLoadTrigger::Force, &mut work)
        .unwrap()
        .unwrap();
    assert!(cells.grids().explicit_lock(forced).unwrap());
}
#[test]
fn linked_ngrids_unlink_in_place_and_reconstruction_gets_a_fresh_generation() {
    let mut cells = cells();
    let a = cells.ensure_grid(1, 2).unwrap();
    let b = cells.ensure_grid(2, 2).unwrap();
    let c = cells.ensure_grid(3, 2).unwrap();
    assert_eq!(cells.grids().linked().collect::<Vec<_>>(), [c, b, a]);
    assert_eq!(cells.ensure_grid(1, 2).unwrap(), a);
    cells.destroy_empty_grid(b).unwrap();
    assert_eq!(cells.grids().linked().collect::<Vec<_>>(), [c, a]);
    let next = cells.ensure_grid(2, 2).unwrap();
    assert_ne!(next, b);
    assert_eq!(cells.grids().linked().collect::<Vec<_>>(), [next, c, a]);
    assert!(cells.grids().phase(b).is_err());
}
#[test]
fn missing_reached_owner_is_sticky_without_rolling_back_the_source_loaded_bit() {
    let mut cells = cells();
    let mut work = Work {
        fail: true,
        ..Work::default()
    };
    let first = cells.ensure_loaded(pair(512, 512), &mut work).unwrap_err();
    assert!(cells.object_data_loaded(32, 32));
    assert_eq!(
        cells.ensure_loaded(pair(512, 512), &mut work).unwrap_err(),
        first
    );
    assert_eq!((work.products, work.loads, work.corpses), (1, 1, 0));
    assert!(cells
        .visit(cells.cell(pair(512, 512)).unwrap(), CellContainer::Grid)
        .unwrap()
        .next()
        .is_none());
}
#[test]
fn unbound_configuration_refuses_before_geometry_and_grid_unload_false_locks_new_grids() {
    let mut cells = SourceCells::new(map()).unwrap();
    let mut work = Work::default();
    assert_eq!(
        cells.ensure_loaded(pair(0, 0), &mut work).unwrap_err().id,
        "execution.pending_grid_configuration"
    );
    assert!(cells.grids().identity(0, 0).is_none());
    assert_eq!(work.products, 0);
    cells.configure_grids(60000, false).unwrap();
    let grid = cells.ensure_created(0, 0, &mut work).unwrap();
    assert!(cells.grids().explicit_lock(grid).unwrap());
    assert_eq!(cells.grids().expiry(grid).unwrap(), 60000);
}
struct Definitions {
    read: Vec<u64>,
}
impl GridDefinitions for Definitions {
    type Row = u64;
    fn read(&mut self, attempt: GridLoadAttempt) -> Result<u64> {
        self.read.push(attempt.catalog_ordinal);
        Ok(attempt.catalog_ordinal)
    }
}
#[derive(Default)]
struct Products(usize);
impl GridProductOwner for Products {
    fn load(&mut self, _map: MapIdentity, _coordinates: ProductCoordinates) -> Result<bool> {
        self.0 += 1;
        Ok(true)
    }
}
#[derive(Default)]
struct Factory {
    attempts: Vec<GridLoadAttempt>,
    failed: Vec<ObjectHandle>,
    viewpoints: Vec<u32>,
    bg: Vec<u32>,
    world_cells: Vec<CellPair>,
    restored: Vec<CellPair>,
    nested: Option<CellPair>,
}
impl GridObjectFactory<Definitions, Products> for Factory {
    fn load_from_db(
        &mut self,
        a: GridLoadAttempt,
        row: u64,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<GridCandidateOutcome> {
        assert_eq!(row, a.catalog_ordinal);
        context.load_grid(a.cell, GridLoadTrigger::ActiveObject, self)?;
        let objects = &mut *context.objects;
        let cells = &mut *context.cells;
        assert!(cells.object_data_loaded(a.grid.x, a.grid.y));
        self.attempts.push(a);
        let (kind, high, dbkind) = match a.kind {
            GridSpawnKind::GameObject => (
                SourceObjectKind::GameObject,
                HighGuid::GameObject,
                DatabaseLookupKind::GameObject,
            ),
            GridSpawnKind::Creature => (
                SourceObjectKind::Creature,
                HighGuid::Creature,
                DatabaseLookupKind::Creature,
            ),
        };
        let handle = objects.construct(2, kind)?;
        if a.guid == 9 || !objects.candidate_allowed(a.grid.map, dbkind, a.guid) {
            objects.destroy(handle, cells)?;
            self.failed.push(handle);
            return Ok(GridCandidateOutcome::FailedAndDestroyed(handle));
        }
        objects.assign_guid(handle, a.guid, SourceGuid::with_entry(high, 1, a.guid))?;
        objects.bind_map(handle, a.grid.map, cells)?;
        if a.kind == GridSpawnKind::Creature {
            objects.link_cell(handle, cells.cell(a.cell)?, cells)?;
        }
        objects.add_to_world(handle)?;
        Ok(GridCandidateOutcome::Loaded(handle))
    }
    fn viewpoint_added(
        &mut self,
        h: ObjectHandle,
        _cell: CellIdentity,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        assert!(context.cells.registered_cell(h).is_some());
        self.viewpoints.push(context.objects.get(h)?.database_guid);
        if let Some(entry) = self.nested.take() {
            context.load_grid(entry, GridLoadTrigger::ActiveObject, self)?;
        }
        Ok(())
    }
    fn battleground_db_load(
        &mut self,
        h: ObjectHandle,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.bg.push(context.objects.get(h)?.database_guid);
        Ok(())
    }
    fn cell_corpses(
        &mut self,
        cell: CellPair,
        _context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.world_cells.push(cell);
        Ok(())
    }
    fn resurrectable_corpses(
        &mut self,
        entry: CellPair,
        _context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.restored.push(entry);
        Ok(())
    }
}
#[test]
fn loader_preserves_xy_types_guid_sets_and_duplicate_static_persistent_attempts() {
    let mut cells = cells();
    let mut sets = GridSpawnSets::new(map());
    let origin = pair(512, 512);
    for guid in [9, 7, 2, 7] {
        sets.register(
            origin,
            GridSpawnKind::GameObject,
            GridSpawnOrigin::Static,
            guid,
            u64::from(guid),
        )
        .unwrap();
    }
    for guid in [2, 1] {
        sets.register(
            origin,
            GridSpawnKind::GameObject,
            GridSpawnOrigin::Persistent,
            guid,
            u64::from(guid),
        )
        .unwrap();
    }
    for guid in [8, 1] {
        sets.register(
            origin,
            GridSpawnKind::Creature,
            GridSpawnOrigin::Static,
            guid,
            100 + u64::from(guid),
        )
        .unwrap();
    }
    sets.register(
        pair(512, 513),
        GridSpawnKind::GameObject,
        GridSpawnOrigin::Static,
        4,
        4,
    )
    .unwrap();
    sets.register(
        pair(512, 513),
        GridSpawnKind::Creature,
        GridSpawnOrigin::Static,
        6,
        106,
    )
    .unwrap();
    sets.register(
        pair(513, 512),
        GridSpawnKind::GameObject,
        GridSpawnOrigin::Static,
        3,
        3,
    )
    .unwrap();
    sets.register(
        pair(0, 0),
        GridSpawnKind::GameObject,
        GridSpawnOrigin::Static,
        99,
        99,
    )
    .unwrap();
    let mut definitions = Definitions { read: Vec::new() };
    let mut factory = Factory::default();
    let mut products = Products::default();
    let mut objects = SourceObjects::default();
    let mut loader = SourceGridLoader {
        definitions: &mut definitions,
        factory: &mut factory,
        products: &mut products,
        sets: &mut sets,
        objects: &mut objects,
    };
    assert!(cells.ensure_loaded(origin, &mut loader).unwrap());
    assert_eq!(definitions.read, [2, 7, 9, 1, 2, 101, 108, 4, 106, 3]);
    assert_eq!(
        factory.attempts.iter().map(|a| a.guid).collect::<Vec<_>>(),
        [2, 7, 9, 1, 2, 1, 8, 4, 6, 3]
    );
    assert_eq!(factory.attempts[4].origin, GridSpawnOrigin::Persistent);
    assert_eq!(factory.failed.len(), 2);
    for h in &factory.failed {
        assert!(objects.get(*h).is_err());
    }
    assert_eq!(factory.world_cells.len(), 256);
    assert_eq!(
        &factory.world_cells[..3],
        &[pair(512, 512), pair(512, 513), pair(512, 514)]
    );
    assert_eq!(factory.world_cells[16], pair(513, 512));
    assert_eq!(factory.restored, [origin]);
    assert_eq!(factory.viewpoints, factory.bg);
    let resident = cells
        .visit(cells.cell(origin).unwrap(), CellContainer::Grid)
        .unwrap()
        .map(|h| objects.get(h).unwrap().database_guid)
        .collect::<Vec<_>>();
    assert_eq!(resident, [1, 7, 2, 8, 1]);
    assert_eq!(products.0, 1);
}
#[test]
fn camera_small_ranges_visit_standing_first_then_xy_and_both_container_passes() {
    let mut cells = cells();
    let mut work = Work::default();
    let standing = compute_cell_pair(0, 0).unwrap();
    let mut seen = Vec::new();
    let request = CameraCellVisit {
        standing,
        x_bits: 0,
        y_bits: 0,
        radius_bits: 1.0f32.to_bits(),
        bounding_radius_bits: 0,
        no_create: false,
    };
    cells
        .visit_camera(request, &mut work, |_, _, cell, kind| {
            seen.push((cell.x, cell.y, kind));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        seen,
        [
            (512, 512, CellContainer::Grid),
            (511, 511, CellContainer::Grid),
            (511, 512, CellContainer::Grid),
            (512, 511, CellContainer::Grid),
            (512, 512, CellContainer::World),
            (511, 511, CellContainer::World),
            (511, 512, CellContainer::World),
            (512, 511, CellContainer::World)
        ]
    );
    assert_eq!(work.loads, 4);
    let before = (work.products, work.loads);
    seen.clear();
    let cold = CameraCellVisit {
        standing: pair(0, 0),
        x_bits: (-17000.0f32).to_bits(),
        y_bits: (-17000.0f32).to_bits(),
        radius_bits: 0,
        bounding_radius_bits: 0,
        no_create: true,
    };
    cells
        .visit_camera(cold, &mut work, |_, _, _, _| {
            panic!("NoCreate cannot visit a cold grid")
        })
        .unwrap();
    assert_eq!((work.products, work.loads), before);
    assert!(cells.grids().identity(0, 0).is_none());
}
#[test]
fn camera_octagon_preserves_strip_order_bounding_radius_and_source_cap() {
    let standing = compute_cell_pair(0, 0).unwrap();
    let small = source_cell_visit(standing, 0, 0, 1.0f32.to_bits(), 0)
        .unwrap()
        .collect::<Vec<_>>();
    let bounded = source_cell_visit(standing, 0, 0, 0, 40.0f32.to_bits())
        .unwrap()
        .collect::<Vec<_>>();
    assert!(bounded.len() > small.len());
    let circle = source_cell_visit(standing, 0, 0, 100.0f32.to_bits(), 0)
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(circle.len(), 52);
    assert_eq!(
        &circle[..3],
        &[pair(510, 508), pair(510, 509), pair(510, 510)]
    );
    assert_eq!(
        &circle[32..36],
        &[
            pair(509, 514),
            pair(514, 514),
            pair(509, 513),
            pair(514, 513)
        ]
    );
    assert!(!circle.contains(&pair(508, 508)));
    assert_ne!(circle[0], standing);
    assert_eq!(
        circle
            .iter()
            .map(|c| (c.x(), c.y()))
            .collect::<BTreeSet<_>>()
            .len(),
        circle.len()
    );
    let maximum = source_cell_visit(standing, 0, 0, 533.3333f32.to_bits(), 0)
        .unwrap()
        .collect::<Vec<_>>();
    let capped = source_cell_visit(standing, 0, 0, 1000.0f32.to_bits(), 100.0f32.to_bits())
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(maximum, capped);
    assert_eq!(
        source_cell_visit(standing, 0, 0, (-1.0f32).to_bits(), 0)
            .unwrap()
            .collect::<Vec<_>>(),
        [standing]
    );
}

#[test]
fn player_entry_registers_the_player_even_when_the_grid_was_already_object_loaded() {
    let mut cells = cells();
    let mut work = Work::default();
    let entry = pair(512, 512);
    cells
        .trigger_grid_load(entry, GridLoadTrigger::ActiveObject, &mut work)
        .unwrap();
    let grid = cells.grids().identity(32, 32).unwrap();
    assert_eq!(cells.grids().phase(grid).unwrap(), NGridPhase::Idle);
    let mut objects = SourceObjects::default();
    let player = objects.construct(1, SourceObjectKind::Player).unwrap();
    objects
        .assign_guid(player, 0, SourceGuid::without_entry(HighGuid::Player, 7))
        .unwrap();
    objects.bind_map(player, map(), &cells).unwrap();
    objects
        .enter_player_grid(player, entry, &mut cells, &mut work)
        .unwrap();
    assert_eq!(
        cells.registered_cell(player),
        Some(cells.cell(entry).unwrap())
    );
    assert_eq!(work.loads, 1);
    assert_eq!(cells.grids().phase(grid).unwrap(), NGridPhase::Idle);
    let request = CameraCellVisit::visibility(0, 0, 0, 10.0f32.to_bits(), true).unwrap();
    assert_eq!(request.radius_bits, 0x4405_5555);
    assert!(!request.no_create);
}

#[test]
fn maximum_camera_visit_is_the_source_octagon_not_a_nine_grid_rectangle() {
    let standing = compute_cell_pair(0, 0).unwrap();
    let visits = source_cell_visit(standing, 0, 0, 0x4405_5555, 0)
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(visits.len(), 869);
    assert!(visits.contains(&pair(528, 512)));
    assert!(!visits.contains(&pair(528, 528)));
    let grids = visits
        .iter()
        .map(|c| (c.grid_x(), c.grid_y()))
        .collect::<BTreeSet<_>>();
    assert_eq!(grids.len(), 8);
    assert!(!grids.contains(&(33, 33)));
}

#[test]
fn real_factory_callbacks_can_load_another_grid_through_the_same_owners() {
    let mut cells = cells();
    let mut definitions = Definitions { read: Vec::new() };
    let mut factory = Factory {
        nested: Some(pair(528, 512)),
        ..Factory::default()
    };
    let mut products = Products::default();
    let mut objects = SourceObjects::default();
    let mut sets = GridSpawnSets::new(map());
    sets.register(
        pair(512, 512),
        GridSpawnKind::GameObject,
        GridSpawnOrigin::Static,
        2,
        2,
    )
    .unwrap();
    sets.register(
        pair(512, 512),
        GridSpawnKind::GameObject,
        GridSpawnOrigin::Static,
        7,
        7,
    )
    .unwrap();
    sets.register(
        pair(528, 512),
        GridSpawnKind::Creature,
        GridSpawnOrigin::Static,
        3,
        103,
    )
    .unwrap();
    let mut loader = SourceGridLoader {
        definitions: &mut definitions,
        factory: &mut factory,
        products: &mut products,
        sets: &mut sets,
        objects: &mut objects,
    };
    cells.ensure_loaded(pair(512, 512), &mut loader).unwrap();
    assert_eq!(definitions.read, [2, 103, 7]);
    assert_eq!(products.0, 2);
    assert_eq!(factory.viewpoints, [2, 3, 7]);
    assert_eq!(factory.bg, [3, 2, 7]);
    assert_eq!(factory.world_cells.len(), 512);
    assert!(cells.object_data_loaded(32, 32) && cells.object_data_loaded(33, 32));
}
