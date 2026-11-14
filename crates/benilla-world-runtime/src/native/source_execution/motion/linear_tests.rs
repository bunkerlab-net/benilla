use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
struct Owner {
    formations: SourceFormations,
}
impl MotionExecutionOwner for Owner {
    fn linear_waypoint_endpoint(&mut self, unit: ObjectHandle, _: bool) -> Result<()> {
        // Explicit authored no-slot binding, not an implicit runtime fallback.
        assert!(self.formations.binding(unit).is_none());
        Ok(())
    }
}
fn loaded(points: &[u32]) -> SourceWaypointGenerator {
    let paths = SourceWaypointPaths::load(
        [
            points
                .iter()
                .map(|&point| SourceWaypointRow {
                    ordinal: point.into(),
                    owner: 7,
                    path: 0,
                    point,
                    node: SourceWaypointNode {
                        position: [0.; 3],
                        orientation: None,
                        delay: 0,
                        script: 0,
                    },
                })
                .collect(),
            vec![],
            vec![],
        ],
        &BTreeSet::from([7]),
        &BTreeSet::from([901]),
        None,
    )
    .unwrap();
    let g = SourceWaypointGenerator::linear();
    g.load_path(&paths, 901, 7, 0, WaypointOrigin::None)
        .unwrap();
    g
}
fn master() -> SourceMotionMaster {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 4,
            default_path: 0,
        },
    )
}
#[test]
fn linear_reuses_waypoint_kind_and_drives_noncontiguous_nodes_both_ways() {
    // WaypointMovementGenerator.h:146-150; .cpp:617-646.
    let g = loaded(&[7, 23, 99]);
    let mut owner = Owner::default();
    let master = master();
    assert_eq!(g.kind(), 2);
    let mut key = 7;
    for (expected, returning) in [(23, false), (99, false), (23, true), (7, true), (23, false)] {
        key = g
            .switch_node(&master, &mut owner, &g.path().unwrap().unwrap(), key)
            .unwrap();
        assert_eq!((key, g.drive_way_back()), (expected, returning));
    }
}
#[test]
fn linear_lookahead_reverses_direction_only_for_looped_queries() {
    // WaypointMovementGenerator.cpp:651-667.
    let g = loaded(&[7, 23, 99]);
    assert_eq!(g.node_after(23, false).unwrap(), Some(99));
    assert_eq!(g.node_after(99, false).unwrap(), None);
    assert_eq!(g.node_after(23, true).unwrap(), Some(7));
    assert_eq!(g.node_after(7, true).unwrap(), None);
    g.drive_way_back.set(true);
    assert_eq!(g.node_after(23, false).unwrap(), Some(7));
    assert_eq!(g.node_after(7, false).unwrap(), None);
    assert_eq!(g.node_after(23, true).unwrap(), Some(99));
}
#[test]
fn single_node_reverse_refuses_previous_begin_before_changing_direction() {
    // WaypointMovementGenerator.cpp:624 is undefined on a one-node path.
    let g = loaded(&[7]);
    assert_eq!(
        g.switch_node(
            &master(),
            &mut Owner::default(),
            &g.path().unwrap().unwrap(),
            7
        )
        .unwrap_err()
        .id,
        "execution.formation.waypoint_previous_begin_undefined"
    );
    assert!(!g.drive_way_back());
    let g = loaded(&[7, 23]);
    assert_eq!(
        g.switch_node(
            &master(),
            &mut Owner::default(),
            &g.path().unwrap().unwrap(),
            23
        )
        .unwrap(),
        7
    );
    assert!(g.drive_way_back());
}
#[test]
fn returning_single_node_advances_to_end_after_resetting_direction() {
    // WaypointMovementGenerator.cpp:638-642 then caller dereference :252.
    let g = loaded(&[7]);
    g.drive_way_back.set(true);
    assert_eq!(
        g.switch_node(
            &master(),
            &mut Owner::default(),
            &g.path().unwrap().unwrap(),
            7
        )
        .unwrap_err()
        .id,
        "execution.formation.waypoint_next_end_undefined"
    );
    assert!(!g.drive_way_back());
    let g = loaded(&[7, 23]);
    g.drive_way_back.set(true);
    assert_eq!(
        g.switch_node(
            &master(),
            &mut Owner::default(),
            &g.path().unwrap().unwrap(),
            7
        )
        .unwrap(),
        23
    );
    assert!(!g.drive_way_back());
}
#[test]
fn missing_formation_binding_refuses_without_inventing_absence() {
    struct Missing;
    impl MotionExecutionOwner for Missing {}
    let g = loaded(&[7, 23]);
    assert_eq!(
        g.switch_node(&master(), &mut Missing, &g.path().unwrap().unwrap(), 23)
            .unwrap_err()
            .id,
        "execution.waypoint.formation_linear_endpoint_owner"
    );
}

#[test]
fn interior_nodes_advance_without_a_formation_owner() {
    // No GetFormationSlot query in the non-endpoint branches, cpp:632,645.
    struct Missing;
    impl MotionExecutionOwner for Missing {}
    let g = loaded(&[7, 23, 99]);
    let path = g.path().unwrap().unwrap();
    assert_eq!(
        g.switch_node(&master(), &mut Missing, &path, 7).unwrap(),
        23
    );
    g.drive_way_back.set(true);
    assert_eq!(
        g.switch_node(&master(), &mut Missing, &path, 99).unwrap(),
        23
    );
}

#[test]
fn endpoint_refusal_preserves_direction_flip_before_missing_hook() {
    struct Missing;
    impl MotionExecutionOwner for Missing {}
    let g = loaded(&[7, 23]);
    let path = g.path().unwrap().unwrap();
    assert_eq!(
        g.switch_node(&master(), &mut Missing, &path, 23)
            .unwrap_err()
            .id,
        "execution.waypoint.formation_linear_endpoint_owner"
    );
    assert!(g.drive_way_back());
    assert_eq!(
        g.switch_node(&master(), &mut Missing, &path, 7)
            .unwrap_err()
            .id,
        "execution.waypoint.formation_linear_endpoint_owner"
    );
    assert!(!g.drive_way_back());
}

#[test]
fn linear_arrival_reuses_script_timer_and_waypoint_notification_order() {
    // WaypointMovementGenerator.cpp:183-214,252-253 applies unchanged to Linear.
    let g = loaded(&[7, 23]);
    let mut nodes = (**g.path.borrow().as_ref().unwrap()).clone();
    nodes.get_mut(&7).unwrap().script = 88;
    nodes.get_mut(&7).unwrap().delay = 10;
    *g.path.borrow_mut() = Some(Rc::new(nodes));
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&super::super::super::motion_master_tests::template()),
        )
        .unwrap();
    units.get_mut(&objects, unit).unwrap().has_ai = true;
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 4,
            default_path: 0,
        },
    );
    struct ArrivalOwner(Vec<String>);
    impl MotionExecutionOwner for ArrivalOwner {
        fn waypoint_script(
            &mut self,
            g: &SourceWaypointGenerator,
            _: &mut SourceMotionMaster,
            _: &mut MotionContext<'_>,
            id: u32,
            _: Option<SourceGuid>,
        ) -> Result<()> {
            self.0.push(format!("script:{id}"));
            g.add_pause(5, false);
            Ok(())
        }
        fn waypoint_inform(
            &mut self,
            _: &SourceWaypointGenerator,
            _: &mut SourceMotionMaster,
            _: &mut MotionContext<'_>,
            kind: u32,
            point: u32,
        ) -> Result<()> {
            self.0.push(format!("inform:{kind}:{point}"));
            Ok(())
        }
        fn waypoint_position(&mut self, _: ObjectHandle) -> Result<[f32; 4]> {
            self.0.push("position".into());
            Ok([1.; 4])
        }
        fn spline_finalized(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(true)
        }
    }
    let mut owner = ArrivalOwner(Vec::new());
    g.arrived(
        &mut master,
        &mut MotionContext {
            objects: &objects,
            units: &mut units,
        },
        &mut owner,
    )
    .unwrap();
    assert_eq!(owner.0, ["script:88", "inform:2:7", "position"]);
    assert_eq!(
        (
            g.progress().current,
            g.progress().last_reached,
            g.progress().wait,
            g.progress().script_time
        ),
        (23, 7, 15, 0)
    );
    assert!(!g.drive_way_back());
}

#[test]
fn linear_default_and_explicit_replacement_keep_source_kind_quirk() {
    // MotionMaster.cpp:83-94,470,496. Linear's kind-2 guard asymmetry is intentional.
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&super::super::super::motion_master_tests::template()),
        )
        .unwrap();
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 4,
            default_path: 0,
        },
    );
    struct MissingPaths;
    impl MotionExecutionOwner for MissingPaths {
        fn waypoint_formation_path(&mut self, _: ObjectHandle) -> Result<Option<u32>> {
            Ok(None)
        }
        fn motion_call(
            &mut self,
            _: &mut SourceMotionMaster,
            _: MotionCall,
            _: &mut MotionContext<'_>,
        ) -> Result<()> {
            Ok(())
        }
    }
    let mut owner = MissingPaths;
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        master.initialize(&mut context, &mut owner).unwrap_err().id,
        "execution.waypoint.loaded_waypoint_manager_owner"
    );
    assert_eq!((master.current_type(), master.len()), (2, 1));
    master
        .move_waypoint(
            WaypointPathRequest::default(),
            0,
            None,
            &mut context,
            &mut owner,
        )
        .unwrap();
    assert_eq!(master.len(), 1);
    assert_eq!(
        master
            .move_linear_waypoint(
                WaypointPathRequest {
                    path_id: 9,
                    ..Default::default()
                },
                0,
                None,
                &mut context,
                &mut owner
            )
            .unwrap_err()
            .id,
        "execution.waypoint.loaded_waypoint_manager_owner"
    );
    assert_eq!(
        (
            master.current_type(),
            master.len(),
            master.path_id().unwrap()
        ),
        (2, 2, 9)
    );
}
