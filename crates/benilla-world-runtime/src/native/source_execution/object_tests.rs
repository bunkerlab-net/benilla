// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::resident_map::{compute_cell_pair, CellPair};
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn pair(x: f32, y: f32) -> CellPair {
    compute_cell_pair(x.to_bits(), y.to_bits()).unwrap()
}
fn setup() -> (SourceObjects, SourceCells, CellIdentity) {
    let mut cells = SourceCells::new(map()).unwrap();
    let p = pair(0., 0.);
    let grid = cells.ensure_grid(p.grid_x(), p.grid_y()).unwrap();
    cells.set_object_data_loaded(grid).unwrap();
    let cell = cells.cell(p).unwrap();
    (SourceObjects::default(), cells, cell)
}
fn guid(kind: SourceObjectKind, low: u32) -> SourceGuid {
    match kind {
        SourceObjectKind::Creature => SourceGuid::with_entry(HighGuid::Creature, 47, low),
        SourceObjectKind::Pet => SourceGuid::with_entry(HighGuid::Pet, 47, low),
        SourceObjectKind::GameObject => SourceGuid::with_entry(HighGuid::GameObject, 48, low),
        SourceObjectKind::Transport => SourceGuid::with_entry(HighGuid::Transport, 48, low),
        SourceObjectKind::DynamicObject => SourceGuid::without_entry(HighGuid::DynamicObject, low),
        SourceObjectKind::Corpse | SourceObjectKind::Bones => {
            SourceGuid::without_entry(HighGuid::Corpse, low)
        }
        SourceObjectKind::Player => SourceGuid::without_entry(HighGuid::Player, low),
        SourceObjectKind::Item | SourceObjectKind::Container => {
            SourceGuid::without_entry(HighGuid::Item, low)
        }
        SourceObjectKind::Camera => SourceGuid::default(),
    }
}
fn spawn(
    objects: &mut SourceObjects,
    cells: &mut SourceCells,
    cell: CellIdentity,
    kind: SourceObjectKind,
    low: u32,
) -> ObjectHandle {
    let handle = objects.construct(1, kind).unwrap();
    if kind != SourceObjectKind::Camera {
        objects.assign_guid(handle, low, guid(kind, low)).unwrap();
    }
    objects.bind_map(handle, map(), cells).unwrap();
    objects.link_cell(handle, cell, cells).unwrap();
    if kind != SourceObjectKind::Camera {
        objects.add_to_world(handle).unwrap();
    }
    if kind == SourceObjectKind::Player {
        objects.register_player_lookup(handle).unwrap();
    }
    handle
}
#[test]
fn source_type_lists_front_insert_and_typed_live_lookup() {
    let (mut objects, mut cells, cell) = setup();
    let go_a = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::GameObject,
        9,
    );
    let go_b = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::GameObject,
        2,
    );
    let creature_a = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        8,
    );
    let creature_b = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        3,
    );
    let dynamic = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::DynamicObject,
        4,
    );
    let bones = spawn(&mut objects, &mut cells, cell, SourceObjectKind::Bones, 5);
    let player = spawn(&mut objects, &mut cells, cell, SourceObjectKind::Player, 6);
    let pet = spawn(&mut objects, &mut cells, cell, SourceObjectKind::Pet, 7);
    let corpse = spawn(&mut objects, &mut cells, cell, SourceObjectKind::Corpse, 10);
    let camera = spawn(&mut objects, &mut cells, cell, SourceObjectKind::Camera, 0);
    assert_eq!(
        cells
            .visit(cell, CellContainer::Grid)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![go_b, go_a, creature_b, creature_a, dynamic, bones]
    );
    assert_eq!(
        cells
            .visit(cell, CellContainer::World)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![player, pet, corpse, camera]
    );
    assert_eq!(
        objects.lookup(map(), LiveLookupKind::Pet, guid(SourceObjectKind::Pet, 7)),
        Some(pet)
    );
    assert_eq!(
        objects.lookup(
            map(),
            LiveLookupKind::Creature,
            guid(SourceObjectKind::Pet, 7)
        ),
        None
    );
    assert_eq!(
        objects.lookup(
            map(),
            LiveLookupKind::Player,
            guid(SourceObjectKind::Player, 6)
        ),
        Some(player)
    );
    assert_eq!(
        objects.lookup(
            map(),
            LiveLookupKind::Corpse,
            guid(SourceObjectKind::Corpse, 10)
        ),
        Some(corpse)
    );
    assert_eq!(objects.get(player).unwrap().current_cell, None);
    assert_eq!(objects.get(pet).unwrap().current_cell, Some(cell));
    assert_eq!(objects.get(camera).unwrap().guid, None);
    assert!(objects.destroy(creature_a, &mut cells).is_err());
    objects.remove_from_world(creature_a).unwrap();
    assert_eq!(cells.registered_cell(creature_a), Some(cell));
    objects.destroy(creature_a, &mut cells).unwrap();
    assert!(objects.get(creature_a).is_err());
    assert_eq!(
        cells
            .visit(cell, CellContainer::Grid)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![go_b, go_a, creature_b, dynamic, bones]
    );
    objects.destroy(camera, &mut cells).unwrap();
    assert_eq!(
        cells
            .visit(cell, CellContainer::World)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![player, pet, corpse]
    );
}
#[test]
fn relocation_preserves_same_cell_order_and_requires_real_loaded_destination() {
    let (mut objects, mut cells, cell) = setup();
    let a = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        9,
    );
    let b = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        2,
    );
    let unchanged = objects
        .relocate_creature(a, pair(1., 1.), &mut cells, |_, _, _| {
            panic!("same-cell relocation cannot load")
        })
        .unwrap();
    assert_eq!(unchanged, CellRelocation::SameCell);
    assert_eq!(
        cells
            .visit(cell, CellContainer::Grid)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![b, a]
    );
    let moved = objects
        .relocate_creature(a, pair(34., 0.), &mut cells, |_, _, _| {
            panic!("same-grid relocation cannot load")
        })
        .unwrap();
    assert!(matches!(moved, CellRelocation::Moved { .. }));
    assert_eq!(
        cells
            .visit(cell, CellContainer::Grid)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![b]
    );
    assert_eq!(
        objects
            .relocate_creature(a, pair(600., 0.), &mut cells, |_, _, _| panic!(
                "inactive unloaded destination must refuse first"
            ))
            .unwrap(),
        CellRelocation::RefusedUnloaded
    );
    let before = objects.get(a).unwrap().current_cell;
    objects.bind_source_allocation(a, 4096, 128, 0).unwrap();
    objects.get_mut(a).unwrap().active = true;
    assert!(objects
        .relocate_creature(a, pair(600., 0.), &mut cells, |_, _, _| Err(Refusal {
            id: "execution.pending_actual_grid_loader".into()
        }))
        .is_err());
    assert_eq!(objects.get(a).unwrap().current_cell, before);
    let result = objects
        .relocate_creature(a, pair(600., 0.), &mut cells, |objects, cells, p| {
            let grid = cells.ensure_grid(p.grid_x(), p.grid_y())?;
            cells.set_object_data_loaded(grid)?;
            let loaded = objects.construct(2, SourceObjectKind::Creature)?;
            objects.assign_guid(loaded, 100, guid(SourceObjectKind::Creature, 100))?;
            objects.bind_map(loaded, map(), cells)?;
            objects.link_cell(loaded, cells.cell(p)?, cells)?;
            objects.add_to_world(loaded)
        })
        .unwrap();
    let CellRelocation::Moved { old, new } = result else {
        panic!()
    };
    assert_ne!(old.grid, new.grid);
    assert_eq!(objects.get(a).unwrap().current_cell, Some(new));
    assert_eq!(cells.registered_cell(a), Some(new));
    let registered: Vec<_> = cells.visit(new, CellContainer::Grid).unwrap().collect();
    assert_eq!(registered.len(), 2);
    assert_eq!(registered[0], a);
    assert_eq!(objects.get(registered[1]).unwrap().database_guid, 100);
}

#[test]
fn active_grid_relocation_refuses_unbound_allocations() {
    let (mut objects, mut cells, cell) = setup();
    let object = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        9,
    );
    objects.get_mut(object).unwrap().active = true;
    let before = objects.get(object).unwrap().current_cell;
    assert_eq!(
        objects
            .relocate_creature(object, pair(34., 0.), &mut cells, |_, _, _| {
                panic!("same-grid relocation cannot load")
            })
            .unwrap_err()
            .id,
        "execution.residency.grid_activity_allocation"
    );
    assert_eq!(objects.get(object).unwrap().current_cell, before);
    assert_eq!(cells.registered_cell(object), Some(cell));
    assert_eq!(
        cells
            .visit(cell, CellContainer::Grid)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![object]
    );
}
#[test]
fn no_create_uses_object_data_and_grid_reconstruction_changes_cell_identity() {
    let (mut objects, mut cells, cell) = setup();
    let p = pair(600., 0.);
    let grid = cells.ensure_grid(p.grid_x(), p.grid_y()).unwrap();
    assert!(cells
        .visit_no_create(p, CellContainer::Grid)
        .unwrap()
        .is_none());
    cells.set_object_data_loaded(grid).unwrap();
    assert_eq!(
        cells
            .visit_no_create(p, CellContainer::Grid)
            .unwrap()
            .unwrap()
            .count(),
        0
    );
    let object = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        7,
    );
    assert!(cells.destroy_empty_grid(cell.grid).is_err());
    objects.remove_from_world(object).unwrap();
    objects.unlink_cell(object, &mut cells).unwrap();
    cells.destroy_empty_grid(cell.grid).unwrap();
    let reconstructed = cells.ensure_grid(cell.grid.x, cell.grid.y).unwrap();
    assert_ne!(reconstructed.generation, cell.grid.generation);
    assert!(objects.link_cell(object, cell, &mut cells).is_err());
    assert!(cells.visit(cell, CellContainer::Grid).is_err());
    let new_cell = cells.cell(pair(0., 0.)).unwrap();
    objects.link_cell(object, new_cell, &mut cells).unwrap();
    assert_eq!(objects.get(object).unwrap().current_cell, Some(new_cell));
    assert_eq!(objects.get(object).unwrap().handle, object);
}
#[test]
fn legacy_life_static_guid_reuse_and_failed_factory_are_different() {
    let (mut objects, mut cells, cell) = setup();
    let first = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        7,
    );
    let duplicate = objects.construct(2, SourceObjectKind::Creature).unwrap();
    assert_eq!(objects.get(duplicate).unwrap().guid, None);
    assert!(!objects.candidate_allowed(map(), DatabaseLookupKind::Creature, 7));
    objects.destroy(duplicate, &mut cells).unwrap();
    let failed = objects.construct(2, SourceObjectKind::Creature).unwrap();
    assert_eq!(failed.construction, duplicate.construction + 1);
    let mut allocator = GuidGenerator::new(HighGuid::Creature, 100);
    objects
        .assign_guid(
            failed,
            8,
            SourceGuid::with_entry(HighGuid::Creature, 47, allocator.generate().counter),
        )
        .unwrap();
    objects.destroy(failed, &mut cells).unwrap();
    assert_eq!(allocator.generate().counter, 101);
    objects.mark_dead(first).unwrap();
    assert!(objects.candidate_allowed(map(), DatabaseLookupKind::Creature, 7));
    objects.legacy_respawn(first).unwrap();
    assert_eq!(objects.get(first).unwrap().life_generation, 2);
    assert_eq!(
        objects.get(first).unwrap().guid,
        Some(guid(SourceObjectKind::Creature, 7))
    );
    objects.remove_from_world(first).unwrap();
    objects.destroy(first, &mut cells).unwrap();
    let second = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        7,
    );
    assert_ne!(second, first);
    assert_eq!(objects.get(second).unwrap().life_generation, 1);
    assert!(objects.get(first).is_err());
    assert_eq!(
        objects.lookup(
            map(),
            LiveLookupKind::Creature,
            guid(SourceObjectKind::Creature, 7)
        ),
        Some(second)
    );
}
#[test]
fn database_lookup_prioritizes_alive_without_reordering_construction_or_guid() {
    let (mut objects, mut cells, cell) = setup();
    let dead = spawn(
        &mut objects,
        &mut cells,
        cell,
        SourceObjectKind::Creature,
        7,
    );
    objects.mark_dead(dead).unwrap();
    let alive = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(alive, 7, guid(SourceObjectKind::Creature, 100))
        .unwrap();
    objects.bind_map(alive, map(), &cells).unwrap();
    objects.link_cell(alive, cell, &mut cells).unwrap();
    objects.add_to_world(alive).unwrap();
    assert_eq!(
        objects.lookup_database(map(), DatabaseLookupKind::Creature, 7),
        Some(alive)
    );
    objects.mark_dead(alive).unwrap();
    assert_eq!(
        objects.lookup_database(map(), DatabaseLookupKind::Creature, 7),
        Some(dead)
    );
    let collision = objects.construct(3, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(collision, 7, guid(SourceObjectKind::Creature, 7))
        .unwrap();
    objects.bind_map(collision, map(), &cells).unwrap();
    assert_eq!(
        objects.add_to_world(collision).unwrap_err().id,
        "execution.duplicate_live_guid"
    );
    assert!(!objects.get(collision).unwrap().in_world);
    let wrong = MapIdentity {
        creation_generation: 2,
        ..map()
    };
    assert_eq!(
        objects.lookup(
            wrong,
            LiveLookupKind::Creature,
            guid(SourceObjectKind::Creature, 7)
        ),
        None
    );
}
#[test]
fn player_global_lookup_outlives_remove_from_world_but_never_matches_another_map() {
    let (mut objects, mut cells, cell) = setup();
    let player = spawn(&mut objects, &mut cells, cell, SourceObjectKind::Player, 7);
    objects.remove_from_world(player).unwrap();
    assert!(objects.destroy(player, &mut cells).is_err());
    assert_eq!(
        objects.lookup(
            map(),
            LiveLookupKind::Player,
            guid(SourceObjectKind::Player, 7)
        ),
        None
    );
    objects.remove_player_lookup(player).unwrap();
    objects.destroy(player, &mut cells).unwrap();
}
#[test]
fn pose_observations_cannot_author_registration_or_loaded_state() {
    let (mut objects, mut cells, cell) = setup();
    let observation: crate::native::CreaturePoseCell = serde_json::from_value(serde_json::json!({
        "identity":{"database_guid":7,"entry":47,"guid":guid(SourceObjectKind::Creature,7).raw()},
        "npc":{"spawn":7,"generation":999},"cell":{"x":512,"y":512}
    }))
    .unwrap();
    assert_eq!(
        objects.compare_pose_observation(map(), &observation, &cells),
        PoseObservation::NoLiveObject
    );
    assert_eq!(cells.visit(cell, CellContainer::Grid).unwrap().count(), 0);
    let object = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(object, 7, guid(SourceObjectKind::Creature, 7))
        .unwrap();
    objects.bind_map(object, map(), &cells).unwrap();
    objects.add_to_world(object).unwrap();
    assert_eq!(
        objects.compare_pose_observation(map(), &observation, &cells),
        PoseObservation::NoRegistration
    );
    objects.link_cell(object, cell, &mut cells).unwrap();
    assert_eq!(
        objects.compare_pose_observation(map(), &observation, &cells),
        PoseObservation::Matched(object)
    );
    objects
        .relocate_creature(object, pair(34., 0.), &mut cells, |_, _, _| unreachable!())
        .unwrap();
    assert_eq!(
        objects.compare_pose_observation(map(), &observation, &cells),
        PoseObservation::DifferentCell(object)
    );
    assert_eq!(cells.visit(cell, CellContainer::Grid).unwrap().count(), 0);
}
