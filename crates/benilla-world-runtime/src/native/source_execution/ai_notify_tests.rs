use super::*;

struct Owner {
    calls: Vec<&'static str>,
    notify: SourceAiNotifyState,
    radius: Option<f32>,
    fail_visit: bool,
}
impl Default for Owner {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            notify: SourceAiNotifyState {
                detection_range: 20,
                is_player: false,
                alive: false,
                can_aggro: false,
            },
            radius: None,
            fail_visit: false,
        }
    }
}
impl UnitEventOwner for Owner {
    fn forced_despawn(&mut self, _: bool) -> Result<()> {
        self.calls.push("despawn");
        Ok(())
    }
    fn ai_notify(&mut self) -> Result<()> {
        self.calls.push("notify");
        let fail = self.fail_visit;
        execute_source_ai_notify(
            &mut self.notify,
            b"Rate.Creature.Aggro = 2\n",
            |player, radius| {
                assert!(!player);
                self.radius = Some(radius);
                if fail {
                    Err(refuse(
                        "execution.creature.Unit_10942.Cell_VisitAllObjects_owner",
                    ))
                } else {
                    Ok(())
                }
            },
        )
    }
}
#[test]
fn ai_notify_schedule_keeps_existing_unless_forced_and_uses_unit_clock() {
    let mut events = SourceUnitEvents::default();
    let mut owner = Owner::default();
    assert!(!events.is_ai_notify_scheduled());
    events.update(7, &mut owner).unwrap();
    events.schedule_ai_notify(5, false).unwrap();
    let first = events.ai_notify_event().unwrap();
    assert_eq!(events.inventory().queued[0].add_time_ms, 7);
    assert_eq!(events.inventory().queued[0].exec_time_ms, 12);
    events.schedule_ai_notify(1, false).unwrap();
    assert_eq!(events.ai_notify_event(), Some(first));
    assert_eq!(events.inventory().queued.len(), 1);
    events.schedule_ai_notify(9, true).unwrap();
    assert_ne!(events.ai_notify_event(), Some(first));
    assert_eq!(events.inventory().queued.len(), 1);
    assert_eq!(events.inventory().queued[0].exec_time_ms, 16);
    events.update(8, &mut owner).unwrap();
    assert!(owner.calls.is_empty());
    events.update(1, &mut owner).unwrap();
    assert_eq!(owner.calls, ["notify"]);
    assert!(!events.is_ai_notify_scheduled());
    assert!(events.inventory().queued.is_empty());
}
#[test]
fn ai_notify_zero_delay_waits_for_queue_update_and_preserves_shared_order() {
    for notify_first in [false, true] {
        let mut events = SourceUnitEvents::default();
        let mut owner = Owner::default();
        if notify_first {
            events.schedule_ai_notify(1, false).unwrap();
            events.schedule_forced_despawn(1, false).unwrap();
        } else {
            events.schedule_forced_despawn(1, false).unwrap();
            events.schedule_ai_notify(1, false).unwrap();
        }
        events.update(1, &mut owner).unwrap();
        assert_eq!(
            owner.calls,
            if notify_first {
                ["notify", "despawn"]
            } else {
                ["despawn", "notify"]
            }
        );
        events.schedule_ai_notify(0, true).unwrap();
        assert_eq!(owner.calls.len(), 2);
        events.update(0, &mut owner).unwrap();
        assert_eq!(
            owner.calls,
            if notify_first {
                ["notify", "despawn", "notify"]
            } else {
                ["despawn", "notify", "notify"]
            }
        );
    }
}
#[test]
fn ai_notify_abort_and_finalize_follow_distinct_pointer_lifetimes() {
    let mut events = SourceUnitEvents::default();
    let mut owner = Owner::default();
    events.abort_ai_notify_event().unwrap();
    events.schedule_ai_notify(1, false).unwrap();
    events.abort_ai_notify_event().unwrap();
    assert!(!events.is_ai_notify_scheduled());
    assert!(events.inventory().queued.is_empty());
    events.update(1, &mut owner).unwrap();
    assert!(owner.calls.is_empty());
    events.schedule_ai_notify(1, false).unwrap();
    events.finalize_ai_notify_event();
    assert!(!events.is_ai_notify_scheduled());
    assert_eq!(events.inventory().queued.len(), 1);
    events.update(1, &mut owner).unwrap();
    assert_eq!(owner.calls, ["notify"]);
    events.schedule_ai_notify(1, false).unwrap();
    events.cancel(events.ai_notify_event().unwrap()).unwrap();
    events.update(1, &mut owner).unwrap();
    assert!(!events.is_ai_notify_scheduled());
    assert_eq!(owner.calls, ["notify"]);
    for force in [false, true] {
        events.schedule_ai_notify(1, false).unwrap();
        events.kill_all(force).unwrap();
        assert!(!events.is_ai_notify_scheduled());
        assert!(events.inventory().queued.is_empty());
    }
}
#[test]
fn ai_notify_visit_sets_aggro_only_on_fire_and_refuses_before_finalize() {
    let mut events = SourceUnitEvents::default();
    let mut owner = Owner {
        fail_visit: true,
        ..Owner::default()
    };
    owner.notify.alive = true;
    owner.notify.detection_range = 20;
    events.schedule_ai_notify(5, false).unwrap();
    events.update(4, &mut owner).unwrap();
    assert!(!owner.notify.can_aggro);
    assert_eq!(
        events.update(1, &mut owner).unwrap_err().id,
        "execution.creature.Unit_10942.Cell_VisitAllObjects_owner"
    );
    assert!(owner.notify.can_aggro);
    assert_eq!(owner.radius, Some(90.0));
    assert!(events.is_ai_notify_scheduled());
    assert_eq!(
        events.update(1, &mut owner).unwrap_err().id,
        "execution.unit_events.stopped"
    );
}
#[test]
fn ai_notify_radius_and_player_alive_branches_use_recorded_config() {
    for (player, alive, detection, rate, expected) in [
        (false, true, 20, "1", 45.0),
        (false, false, 60, "2", 120.0),
        (true, true, 18, "0", 0.0),
        (true, false, 18, "-1", 45.0),
    ] {
        let mut state = SourceAiNotifyState {
            is_player: player,
            alive,
            detection_range: detection,
            can_aggro: false,
        };
        let mut visited = false;
        execute_source_ai_notify(
            &mut state,
            format!("Rate.Creature.Aggro = {rate}\n").as_bytes(),
            |actual_player, radius| {
                assert_eq!(actual_player, player);
                assert_eq!(radius, expected);
                visited = true;
                Ok(())
            },
        )
        .unwrap();
        assert!(visited);
        assert_eq!(state.can_aggro, !player && alive);
    }
    let mut state = Owner::default().notify;
    assert!(execute_source_ai_notify(&mut state, b"", |_, _| panic!(
        "missing config precedes visit"
    ))
    .is_err());
}

fn cell_fixture() -> (SourceObjects, SourceCells, ObjectHandle, CellIdentity) {
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let mut objects = SourceObjects::default();
    let mut cells = SourceCells::new(map).unwrap();
    let pair = benilla_sim::resident_map::CellPair::from_indices(512, 512).unwrap();
    cells.configure_grids(300000, true).unwrap();
    cells.grids.create(pair.grid_x(), pair.grid_y()).unwrap();
    let cell = cells.cell(pair).unwrap();
    let creature = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects.bind_map(creature, map, &cells).unwrap();
    objects.set_pose(creature, SourcePose([0; 4])).unwrap();
    objects.get_mut(creature).unwrap().alive = true;
    objects.link_cell(creature, cell, &mut cells).unwrap();
    (objects, cells, creature, cell)
}
#[test]
fn ai_notify_live_visit_skips_self_dead_creatures_and_unloaded_cells() {
    for (loaded, alive) in [(false, true), (true, false)] {
        let (mut objects, mut cells, creature, cell) = cell_fixture();
        if loaded {
            cells.set_object_data_loaded(cell.grid).unwrap();
        }
        let other = objects.construct(1, SourceObjectKind::Creature).unwrap();
        objects.bind_map(other, cells.map(), &cells).unwrap();
        objects.set_pose(other, SourcePose([0; 4])).unwrap();
        objects.get_mut(other).unwrap().alive = alive;
        objects.link_cell(other, cell, &mut cells).unwrap();
        let mut can_aggro = false;
        let mut queue = SourceUnitEvents::default();
        queue.schedule_ai_notify(0, false).unwrap();
        queue
            .update(
                0,
                &mut SourceCreatureAiNotifyOwner {
                    object: creature,
                    objects: &objects,
                    cells: &cells,
                    detection_range: 20,
                    bounding_radius_bits: 0,
                    can_aggro: &mut can_aggro,
                    config: b"Rate.Creature.Aggro = 1\n",
                },
            )
            .unwrap();
        assert!(can_aggro);
        assert!(!queue.is_ai_notify_scheduled());
        assert!(queue.inventory().queued.is_empty());
    }
}
#[test]
fn ai_notify_live_visit_reaches_first_unported_notifier_call_after_aggro() {
    for (kind, alive, expected) in [
        (
            SourceObjectKind::Creature,
            true,
            Some("execution.creature.GridNotifiersImpl_178.UnitVisitObjectsNotifierWorker_owner"),
        ),
        (
            SourceObjectKind::Pet,
            true,
            Some("execution.creature.GridNotifiersImpl_178.UnitVisitObjectsNotifierWorker_owner"),
        ),
        (
            SourceObjectKind::Player,
            true,
            Some("execution.creature.GridNotifiersImpl_156.IsTaxiFlying_owner"),
        ),
        (SourceObjectKind::Player, false, None),
    ] {
        let (mut objects, mut cells, creature, cell) = cell_fixture();
        cells.set_object_data_loaded(cell.grid).unwrap();
        let other = objects.construct(1, kind).unwrap();
        objects.bind_map(other, cells.map(), &cells).unwrap();
        objects.set_pose(other, SourcePose([0; 4])).unwrap();
        objects.get_mut(other).unwrap().alive = alive;
        objects.link_cell(other, cell, &mut cells).unwrap();
        let mut can_aggro = false;
        let mut queue = SourceUnitEvents::default();
        queue.schedule_ai_notify(1, false).unwrap();
        let result = queue.update(
            1,
            &mut SourceCreatureAiNotifyOwner {
                object: creature,
                objects: &objects,
                cells: &cells,
                detection_range: 20,
                bounding_radius_bits: 0,
                can_aggro: &mut can_aggro,
                config: b"Rate.Creature.Aggro = 1\n",
            },
        );
        assert!(can_aggro);
        assert_eq!(
            result.err().map(|error| error.id),
            expected.map(str::to_owned)
        );
        assert_eq!(queue.is_ai_notify_scheduled(), expected.is_some());
    }
}
#[test]
fn ai_notify_dead_owner_clears_aggro_and_noncreature_cast_refuses() {
    for kind in [SourceObjectKind::Creature, SourceObjectKind::Player] {
        let (mut objects, mut cells, creature, cell) = cell_fixture();
        cells.set_object_data_loaded(cell.grid).unwrap();
        objects.get_mut(creature).unwrap().alive = false;
        let object = if kind == SourceObjectKind::Creature {
            creature
        } else {
            let player = objects.construct(1, kind).unwrap();
            objects.bind_map(player, cells.map(), &cells).unwrap();
            objects.set_pose(player, SourcePose([0; 4])).unwrap();
            player
        };
        let mut can_aggro = true;
        let result = SourceCreatureAiNotifyOwner {
            object,
            objects: &objects,
            cells: &cells,
            detection_range: 20,
            bounding_radius_bits: 0,
            can_aggro: &mut can_aggro,
            config: b"Rate.Creature.Aggro = 1\n",
        }
        .ai_notify();
        if kind == SourceObjectKind::Creature {
            result.unwrap();
            assert!(!can_aggro);
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.creature.ai_notify_type_binding"
            );
            assert!(can_aggro);
        }
    }
}
#[test]
fn ai_notify_kill_does_not_finalize_and_force_replaces_stale_pointer() {
    let mut queue = SourceUnitEvents::default();
    queue.schedule_ai_notify(1, false).unwrap();
    let id = queue.ai_notify_event().unwrap();
    assert!(queue.kill_event(id).unwrap());
    assert!(queue.is_ai_notify_scheduled());
    queue.schedule_ai_notify(2, false).unwrap();
    assert!(queue.inventory().queued.is_empty());
    queue.schedule_ai_notify(3, true).unwrap();
    assert_eq!(queue.inventory().queued.len(), 1);
    assert_ne!(queue.ai_notify_event(), Some(id));
    let mut owner = Owner::default();
    queue.update(3, &mut owner).unwrap();
    assert_eq!(owner.calls, ["notify"]);
}
