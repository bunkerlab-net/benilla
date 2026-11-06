// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component fixtures, not reference execution or production admission.
use super::*;

fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn handle(n: u64) -> ObjectHandle {
    ObjectHandle {
        thread: 1,
        construction: n,
    }
}
#[test]
fn duplicate_history_and_same_membership_wrong_pinned_order_refuse() {
    let mut set = SourceUpdateSet::new(map()).unwrap();
    assert!(
        set.insert_token(handle(1), 104, UpdateInsertion::Generic)
            .unwrap()
            .inserted
    );
    assert!(
        set.insert_token(handle(2), 208, UpdateInsertion::Creature)
            .unwrap()
            .inserted
    );
    let duplicate = set
        .insert_token(handle(1), 104, UpdateInsertion::Active)
        .unwrap();
    assert!(!duplicate.inserted);
    assert_eq!(
        (
            duplicate.buckets_before,
            duplicate.buckets_after,
            duplicate.bucket
        ),
        (13, 13, 0)
    );
    assert_eq!(set.order().collect::<Vec<_>>(), [handle(2), handle(1)]);
    set.prove_order(&[handle(2), handle(1)]).unwrap();
    assert_eq!(
        set.prove_order(&[handle(1), handle(2)]).unwrap_err().id,
        "execution.update_set.pinned_iteration_mismatch"
    );
}

#[test]
fn rehash_relinks_old_iteration_and_duplicates_do_not_trigger_growth() {
    let mut set = SourceUpdateSet::new(map()).unwrap();
    for n in 1..=13 {
        set.insert_token(handle(n), 104 * n, UpdateInsertion::Generic)
            .unwrap();
    }
    assert_eq!(set.bucket_count(), 13);
    let duplicate = set
        .insert_token(handle(7), 728, UpdateInsertion::Creature)
        .unwrap();
    assert!(!duplicate.inserted);
    assert_eq!(duplicate.buckets_after, 13);
    assert_eq!(
        set.order().collect::<Vec<_>>(),
        (1..=13).rev().map(handle).collect::<Vec<_>>()
    );
    let growth = set
        .insert_token(handle(14), 1456, UpdateInsertion::Active)
        .unwrap();
    assert_eq!((growth.buckets_before, growth.buckets_after), (13, 29));
    assert_eq!(
        set.order().collect::<Vec<_>>(),
        [14, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13].map(handle)
    );
    assert!(set
        .insert_token(handle(15), 1456, UpdateInsertion::Generic)
        .is_err());
    assert!(set
        .insert_token(handle(15), 1457, UpdateInsertion::Generic)
        .is_err());
}

use benilla_sim::resident_map::CellPair;
fn pose(x: f32, y: f32) -> SourcePose {
    SourcePose([x.to_bits(), y.to_bits(), 0, 0])
}
struct Inputs {
    radius: f32,
    farsight: SourceGuid,
}
impl SourceVisitInputs for Inputs {
    fn visibility_distance(&mut self, _: ObjectHandle) -> Result<f32> {
        Ok(self.radius)
    }
    fn farsight_guid(&mut self, _: ObjectHandle) -> Result<SourceGuid> {
        Ok(self.farsight)
    }
}
#[derive(Default)]
struct Updates(Vec<(ObjectHandle, u32)>);
impl SourceObjectUpdateOwner for Updates {
    fn update(
        &mut self,
        object: ObjectHandle,
        diff: u32,
        _: &mut SourceObjects,
        _: &mut SourceCells,
    ) -> Result<()> {
        self.0.push((object, diff));
        Ok(())
    }
}
struct Fixture {
    objects: SourceObjects,
    cells: SourceCells,
    inputs: Inputs,
    events: Vec<SourceUpdateEvent>,
}
impl Fixture {
    fn new(radius: f32) -> Self {
        let mut cells = SourceCells::new(map()).unwrap();
        cells.configure_grids(300000, true).unwrap();
        Self {
            objects: SourceObjects::default(),
            cells,
            inputs: Inputs {
                radius,
                farsight: SourceGuid::from_raw(0),
            },
            events: Vec::new(),
        }
    }
    fn actor(&mut self, kind: SourceObjectKind, at: SourcePose) -> ObjectHandle {
        let object = self.objects.construct(1, kind).unwrap();
        let high = match kind {
            SourceObjectKind::Player => HighGuid::Player,
            SourceObjectKind::Pet => HighGuid::Pet,
            SourceObjectKind::GameObject | SourceObjectKind::Transport => HighGuid::GameObject,
            SourceObjectKind::DynamicObject => HighGuid::DynamicObject,
            SourceObjectKind::Corpse | SourceObjectKind::Bones => HighGuid::Corpse,
            _ => HighGuid::Creature,
        };
        if kind != SourceObjectKind::Camera {
            self.objects
                .assign_guid(
                    object,
                    0,
                    SourceGuid::with_entry(high, 0, object.construction as u32),
                )
                .unwrap();
        }
        self.objects.bind_map(object, map(), &self.cells).unwrap();
        self.objects
            .bind_source_allocation(object, object.construction * 256, 128, 0)
            .unwrap();
        self.objects.set_pose(object, at).unwrap();
        let pair = at.cell().unwrap();
        let grid = self
            .cells
            .ensure_created(pair.grid_x(), pair.grid_y(), &mut AuthoredProducts)
            .unwrap();
        self.cells.set_object_data_loaded(grid).unwrap();
        self.objects
            .link_cell(object, self.cells.cell(pair).unwrap(), &mut self.cells)
            .unwrap();
        if kind != SourceObjectKind::Camera {
            self.objects.add_to_world(object).unwrap();
        }
        if kind == SourceObjectKind::Player {
            self.objects.register_player_lookup(object).unwrap();
            self.objects.link_map_player(object).unwrap();
        }
        object
    }
    fn collect(&mut self, map_radius: f32) -> Result<SourceUpdateSet> {
        collect_source_updates(
            &self.objects,
            &mut self.cells,
            map_radius,
            &mut self.inputs,
            &mut |event| {
                self.events.push(event);
                Ok(())
            },
        )
    }
    fn active(&mut self, object: ObjectHandle, add: bool) {
        let mut definitions = EmptyDefinitions;
        let mut products = NoProducts;
        let mut factory = NoFactory;
        let mut sets = GridSpawnSets::new(map());
        let mut context = GridFactoryContext {
            definitions: &mut definitions,
            products: &mut products,
            sets: &mut sets,
            objects: &mut self.objects,
            cells: &mut self.cells,
        };
        if add {
            context.add_to_active(object, &mut factory).unwrap();
        } else {
            context.remove_from_active(object).unwrap();
        }
    }
}
struct AuthoredProducts;
impl GridLoadOwner for AuthoredProducts {
    fn load_products(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<bool> {
        Ok(true)
    }
    fn load_objects(&mut self, _: &mut SourceCells, _: GridIdentity) -> Result<()> {
        panic!("registration fixtures do not run object loading")
    }
    fn add_resurrectable_corpses(
        &mut self,
        _: &mut SourceCells,
        _: GridIdentity,
        _: CellPair,
    ) -> Result<()> {
        panic!("registration fixtures do not run corpse loading")
    }
}
struct EmptyDefinitions;
impl GridDefinitions for EmptyDefinitions {
    type Row = ();
    fn read(&mut self, _: GridLoadAttempt) -> Result<()> {
        panic!("no authored definitions reached")
    }
}
struct NoProducts;
impl GridProductOwner for NoProducts {
    fn load(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<bool> {
        panic!("loaded grids stay loaded")
    }
}
struct NoFactory;
impl GridObjectFactory<EmptyDefinitions, NoProducts> for NoFactory {
    fn load_from_db(
        &mut self,
        _: GridLoadAttempt,
        _: (),
        _: &mut GridFactoryContext<'_, EmptyDefinitions, NoProducts>,
    ) -> Result<GridCandidateOutcome> {
        panic!("no authored construction reached")
    }
    fn viewpoint_added(
        &mut self,
        _: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<'_, EmptyDefinitions, NoProducts>,
    ) -> Result<()> {
        panic!("no authored viewpoint reached")
    }
    fn battleground_db_load(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<'_, EmptyDefinitions, NoProducts>,
    ) -> Result<()> {
        panic!("no authored battleground reached")
    }
    fn cell_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<'_, EmptyDefinitions, NoProducts>,
    ) -> Result<()> {
        panic!("no authored corpse load reached")
    }
    fn resurrectable_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<'_, EmptyDefinitions, NoProducts>,
    ) -> Result<()> {
        panic!("no authored entry load reached")
    }
}

#[test]
fn overlaps_suppress_cells_not_visibility_and_exclude_unvisited_loaded_actors() {
    let mut fixture = Fixture::new(1.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    // Beyond visibility, but still in the visited cell.
    let nonvisible = fixture.actor(SourceObjectKind::Creature, pose(30.0, 30.0));
    let far = fixture.actor(SourceObjectKind::Creature, pose(3000.0, 3000.0));
    let set = fixture.collect(1.0).unwrap();
    assert_eq!(set.order().collect::<Vec<_>>(), [nonvisible]);
    assert!(!set.order().any(|object| object == far));
    let inserts: Vec<_> = fixture
        .events
        .iter()
        .filter_map(|event| {
            if let SourceUpdateEvent::Insert(receipt) = event {
                Some(receipt)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(inserts.len(), 1);
    assert!(fixture
        .events
        .iter()
        .any(|event| matches!(event, SourceUpdateEvent::SuppressedCell(_))));
    let mut updates = Updates::default();
    set.dispatch(
        137,
        &mut fixture.objects,
        &mut fixture.cells,
        &mut updates,
        &mut |event| {
            fixture.events.push(event);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(updates.0, [(nonvisible, 137)]);
    assert!(fixture
        .events
        .contains(&SourceUpdateEvent::UpdateBegin(nonvisible)));
    assert!(fixture
        .events
        .contains(&SourceUpdateEvent::UpdateEnd(nonvisible)));
}

#[test]
fn no_create_marks_cold_cells_without_creating_grids_and_grid_precedes_world() {
    let mut fixture = Fixture::new(40.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let go = fixture.actor(SourceObjectKind::GameObject, pose(1.0, 1.0));
    let creature = fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0));
    let dynamic = fixture.actor(SourceObjectKind::DynamicObject, pose(1.0, 1.0));
    let pet = fixture.actor(SourceObjectKind::Pet, pose(1.0, 1.0));
    for kind in [
        SourceObjectKind::Corpse,
        SourceObjectKind::Bones,
        SourceObjectKind::Camera,
    ] {
        fixture.actor(kind, pose(1.0, 1.0));
    }
    let set = fixture.collect(40.0).unwrap();
    assert_eq!(set.len(), 4);
    let attempts: Vec<_> = fixture
        .events
        .iter()
        .filter_map(|event| match event {
            SourceUpdateEvent::Insert(receipt) => Some((receipt.object, receipt.overload)),
            _ => None,
        })
        .collect();
    assert_eq!(
        attempts,
        [
            (go, UpdateInsertion::Generic),
            (creature, UpdateInsertion::Creature),
            (dynamic, UpdateInsertion::Generic),
            (pet, UpdateInsertion::Creature)
        ]
    );
    let visits: Vec<_> = fixture
        .events
        .iter()
        .filter_map(|event| match event {
            SourceUpdateEvent::Cell { pair, loaded } => Some((*pair, *loaded)),
            _ => None,
        })
        .collect();
    assert_eq!(visits.len(), 16);
    assert_eq!(visits[0].0, CellPair::from_indices(510, 510).unwrap());
    assert_eq!(visits[1].0, CellPair::from_indices(510, 511).unwrap());
    assert!(visits.iter().any(|(_, loaded)| !loaded));
    for (pair, loaded) in visits {
        if !loaded {
            assert!(!fixture
                .cells
                .object_data_loaded(pair.grid_x(), pair.grid_y()));
            assert!(fixture
                .cells
                .grids()
                .identity(pair.grid_x(), pair.grid_y())
                .is_none());
        }
    }
}

#[test]
fn farsight_uses_resolved_current_world_target_not_a_supplied_membership() {
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let target = fixture.actor(SourceObjectKind::DynamicObject, pose(3000.0, 3000.0));
    let neighbour = fixture.actor(SourceObjectKind::Creature, pose(3001.0, 3001.0));
    fixture.inputs.farsight = fixture.objects.get(target).unwrap().guid.unwrap();
    let set = fixture.collect(0.0).unwrap();
    assert_eq!(set.len(), 2);
    assert!(set.order().any(|object| object == neighbour));
    fixture.objects.remove_from_world(target).unwrap();
    assert!(fixture.collect(0.0).unwrap().is_empty());
}

#[test]
fn remote_active_updates_itself_and_neighbours_with_duplicate_attempts() {
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let active = fixture.actor(SourceObjectKind::Creature, pose(3000.0, 3000.0));
    let neighbour = fixture.actor(SourceObjectKind::Pet, pose(3030.0, 3000.0));
    let far = fixture.actor(SourceObjectKind::Creature, pose(-3000.0, -3000.0));
    fixture.active(active, true);
    let set = fixture.collect(40.0).unwrap();
    assert_eq!(set.len(), 2);
    assert!(set.order().any(|object| object == active));
    assert!(set.order().any(|object| object == neighbour));
    assert!(!set.order().any(|object| object == far));
    let attempts: Vec<_> = fixture
        .events
        .iter()
        .filter_map(|event| match event {
            SourceUpdateEvent::Insert(receipt) if receipt.object == active => {
                Some((receipt.overload, receipt.inserted))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        attempts,
        [
            (UpdateInsertion::Active, true),
            (UpdateInsertion::Creature, false)
        ]
    );
    let mut updates = Updates::default();
    set.dispatch(
        50,
        &mut fixture.objects,
        &mut fixture.cells,
        &mut updates,
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(updates.0.len(), 2);
}

#[test]
fn active_iterator_erases_next_preserves_cursor_and_does_not_restart_at_end() {
    let mut fixture = Fixture::new(0.0);
    let actors: Vec<_> = (0..5)
        .map(|_| fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0)))
        .collect();
    for &index in &[0, 2, 4] {
        fixture.active(actors[index], true);
    }
    fixture.cells.active_mut().begin_visit().unwrap();
    assert_eq!(
        fixture.cells.active_mut().next_member().unwrap(),
        Some(actors[0])
    );
    fixture.active(actors[2], false);
    fixture.active(actors[1], true); // Behind stored cursor, waits for next visit.
    assert_eq!(
        fixture.cells.active_mut().next_member().unwrap(),
        Some(actors[4])
    );
    fixture.active(actors[3], true);
    assert_eq!(fixture.cells.active_mut().next_member().unwrap(), None);
    fixture.cells.active_mut().finish_visit();
    assert_eq!(fixture.collect(0.0).unwrap().len(), 5);
}

#[test]
fn removed_registration_and_invalid_active_position_are_not_collected() {
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let removed = fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0));
    fixture.objects.remove_from_world(removed).unwrap();
    fixture
        .objects
        .unlink_cell(removed, &mut fixture.cells)
        .unwrap();
    let active = fixture.actor(SourceObjectKind::Creature, pose(3000.0, 3000.0));
    fixture.active(active, true);
    fixture
        .objects
        .set_pose(active, SourcePose([f32::NAN.to_bits(), 0, 0, 0]))
        .unwrap();
    assert!(fixture.collect(0.0).unwrap().is_empty());
    fixture
        .objects
        .set_pose(active, pose(3000.0, 3000.0))
        .unwrap();
    fixture.objects.remove_from_world(active).unwrap();
    assert!(fixture.collect(0.0).unwrap().is_empty());
}

#[test]
fn stale_collected_generation_never_redirects_to_reused_allocation() {
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let old = fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0));
    let set = fixture.collect(0.0).unwrap();
    fixture.objects.remove_from_world(old).unwrap();
    fixture.objects.destroy(old, &mut fixture.cells).unwrap();
    let new = fixture
        .objects
        .construct(1, SourceObjectKind::Creature)
        .unwrap();
    fixture
        .objects
        .bind_source_allocation(new, old.construction * 256, 128, 0)
        .unwrap();
    let mut updates = Updates::default();
    assert_eq!(
        set.dispatch(
            50,
            &mut fixture.objects,
            &mut fixture.cells,
            &mut updates,
            &mut |_| Ok(())
        )
        .unwrap_err()
        .id,
        "execution.stale_object_handle"
    );
    assert!(updates.0.is_empty());
    assert!(SourceUpdateSet::new(map())
        .unwrap()
        .insert(&fixture.objects, old, UpdateInsertion::Active)
        .is_err());
}

#[test]
fn world_object_lookup_preserves_registered_before_world_type_semantics() {
    let mut fixture = Fixture::new(0.0);
    for kind in [
        SourceObjectKind::Creature,
        SourceObjectKind::Pet,
        SourceObjectKind::GameObject,
        SourceObjectKind::DynamicObject,
        SourceObjectKind::Player,
        SourceObjectKind::Corpse,
    ] {
        let object = fixture.actor(kind, pose(1.0, 1.0));
        let guid = fixture.objects.get(object).unwrap().guid.unwrap();
        fixture.objects.remove_from_world(object).unwrap();
        fixture.objects.register_world_lookup(object).unwrap();
        let before = fixture.objects.lookup_world_object(map(), guid);
        if matches!(kind, SourceObjectKind::Player | SourceObjectKind::Corpse) {
            assert_eq!(before, None);
        } else {
            assert_eq!(before, Some(object));
        }
        fixture.objects.mark_in_world(object).unwrap();
        assert_eq!(
            fixture.objects.lookup_world_object(map(), guid),
            Some(object)
        );
        assert_eq!(
            fixture.objects.lookup_world_object(
                MapIdentity {
                    creation_generation: 2,
                    ..map()
                },
                guid
            ),
            None
        );
    }
    assert_eq!(
        fixture
            .objects
            .lookup_world_object(map(), SourceGuid::without_entry(HighGuid::MoTransport, 9)),
        None
    );
}

#[test]
fn registered_farsight_before_world_uses_map_radius_and_updater_does_not_refilter() {
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let target = fixture.actor(SourceObjectKind::DynamicObject, pose(3000.0, 3000.0));
    let neighbour = fixture.actor(SourceObjectKind::Creature, pose(3030.0, 3000.0));
    fixture.inputs.farsight = fixture.objects.get(target).unwrap().guid.unwrap();
    fixture.objects.remove_from_world(target).unwrap();
    fixture.objects.register_world_lookup(target).unwrap();
    let set = fixture.collect(40.0).unwrap();
    assert_eq!(set.len(), 2);
    let mut updates = Updates::default();
    set.dispatch(
        100,
        &mut fixture.objects,
        &mut fixture.cells,
        &mut updates,
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(updates.0.contains(&(target, 100)));
    assert!(updates.0.contains(&(neighbour, 100)));
}

#[test]
fn reached_update_refusal_stops_without_an_end_or_later_updates() {
    struct Refuses;
    impl SourceObjectUpdateOwner for Refuses {
        fn update(
            &mut self,
            _: ObjectHandle,
            _: u32,
            _: &mut SourceObjects,
            _: &mut SourceCells,
        ) -> Result<()> {
            Err(Refusal {
                id: "authored.reached_update_owner".into(),
            })
        }
    }
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0));
    fixture.actor(SourceObjectKind::Pet, pose(1.0, 1.0));
    let set = fixture.collect(0.0).unwrap();
    let mut events = Vec::new();
    assert_eq!(
        set.dispatch(
            100,
            &mut fixture.objects,
            &mut fixture.cells,
            &mut Refuses,
            &mut |event| {
                events.push(event);
                Ok(())
            }
        )
        .unwrap_err()
        .id,
        "authored.reached_update_owner"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SourceUpdateEvent::UpdateBegin(_)))
            .count(),
        1
    );
    assert!(!events.iter().any(|event| matches!(
        event,
        SourceUpdateEvent::UpdateEnd(_) | SourceUpdateEvent::TraversalEnd
    )));
}

#[test]
fn no_create_preserves_an_existing_cold_grid_and_marks_reset_each_update() {
    let mut fixture = Fixture::new(40.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let actor = fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0));
    let cold = fixture.cells.ensure_grid(31, 31).unwrap();
    for _ in 0..2 {
        assert_eq!(
            fixture.collect(40.0).unwrap().order().collect::<Vec<_>>(),
            [actor]
        );
        assert_eq!(fixture.cells.grids().identity(31, 31), Some(cold));
        assert!(!fixture.cells.object_data_loaded(31, 31));
    }
}

#[test]
fn unlink_after_collection_does_not_suppress_a_live_source_update() {
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let actor = fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0));
    let set = fixture.collect(0.0).unwrap();
    fixture.objects.remove_from_world(actor).unwrap();
    fixture
        .objects
        .unlink_cell(actor, &mut fixture.cells)
        .unwrap();
    let mut updates = Updates::default();
    set.dispatch(
        50,
        &mut fixture.objects,
        &mut fixture.cells,
        &mut updates,
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(updates.0, [(actor, 50)]);
}

#[test]
fn completed_set_retains_members_but_refuses_later_mutations_and_traversals() {
    let mut fixture = Fixture::new(0.0);
    fixture.actor(SourceObjectKind::Player, pose(1.0, 1.0));
    let actor = fixture.actor(SourceObjectKind::Creature, pose(1.0, 1.0));
    let set = fixture.collect(0.0).unwrap();
    let mut updates = Updates::default();
    let mut completed = set
        .dispatch(
            50,
            &mut fixture.objects,
            &mut fixture.cells,
            &mut updates,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(completed.order().collect::<Vec<_>>(), [actor]);
    assert_eq!(
        completed
            .insert(&fixture.objects, actor, UpdateInsertion::Active)
            .unwrap_err()
            .id,
        "execution.update_set.collection_closed"
    );
    assert_eq!(
        completed
            .dispatch(
                50,
                &mut fixture.objects,
                &mut fixture.cells,
                &mut updates,
                &mut |_| Ok(())
            )
            .unwrap_err()
            .id,
        "execution.update_set.traversal_repeated"
    );
    assert_eq!(updates.0, [(actor, 50)]);
}
