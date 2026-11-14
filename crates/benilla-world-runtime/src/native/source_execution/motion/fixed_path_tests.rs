use super::*;

fn path(delays: &[u32]) -> SourceWaypointPath {
    delays
        .iter()
        .enumerate()
        .map(|(i, &delay)| {
            (
                (i as u32 + 1) * 7,
                SourceWaypointNode {
                    position: [i as f32, 2., 3.],
                    orientation: Some(i as f32),
                    delay,
                    script: 0,
                },
            )
        })
        .collect()
}
#[test]
fn delays_disable_cyclic_and_noncyclic_segments_stop_at_first_delay() {
    // PathMovementGenerator.cpp:48-55,82-95.
    let g = SourceFixedPathGenerator::from_path(
        Some(&path(&[0, 25, 0])),
        FixedPathOptions {
            cyclic: true,
            ..Default::default()
        },
    );
    g.prepare_spline().unwrap();
    assert!(!g.progress().cyclic);
    assert_eq!(g.progress().wait, 25);
    assert_eq!(g.progress().orientation, Some(1.));
    assert_eq!(g.spline.borrow().len(), 3);
}
#[test]
fn cyclic_spline_rotates_at_offset_and_does_not_select_orientation() {
    // PathMovementGenerator.cpp:97-113.
    let g = SourceFixedPathGenerator::from_path(
        Some(&path(&[0, 0, 0])),
        FixedPathOptions {
            offset: 2,
            cyclic: true,
            ..Default::default()
        },
    );
    g.prepare_spline().unwrap();
    assert_eq!(
        &*g.spline.borrow(),
        &[[0.; 3], [2., 2., 3.], [0., 2., 3.], [1., 2., 3.]]
    );
    assert_eq!(g.progress().orientation, None);
}
#[test]
fn offset_at_end_is_valid_but_before_begin_and_past_end_refuse() {
    // PathMovementGenerator.cpp:84,100 std::next on a bidirectional map iterator.
    for offset in [-1, 4] {
        let g = SourceFixedPathGenerator::from_path(
            Some(&path(&[0, 0, 0])),
            FixedPathOptions {
                offset,
                ..Default::default()
            },
        );
        assert_eq!(
            g.prepare_spline().unwrap_err().id,
            "execution.fixed_path.offset_iterator_undefined"
        );
    }
    let g = SourceFixedPathGenerator::from_path(
        Some(&path(&[0, 0, 0])),
        FixedPathOptions {
            offset: 3,
            ..Default::default()
        },
    );
    g.prepare_spline().unwrap();
    assert_eq!(g.spline.borrow().len(), 1);
}
#[test]
fn forced_pause_clears_spline_and_unsigned_pause_addition_wraps() {
    // PathMovementGenerator.cpp:304-316; Timer.h:145-162.
    let g = SourceFixedPathGenerator::from_path(Some(&path(&[9])), FixedPathOptions::default());
    g.prepare_spline().unwrap();
    g.add_pause(-10, false);
    assert_eq!(g.progress().wait, 1);
    g.add_pause(i32::MIN, true);
    assert_eq!(g.progress().wait, 0x8000_0000);
    assert!(g.spline.borrow().is_empty());
    g.add_pause(-1, false);
    assert_eq!(g.progress().wait, i32::MAX as u32);
    g.add_pause(0, true);
    g.add_pause(20, false);
    assert_eq!(g.progress().wait, 0);
}
#[test]
fn point_array_constructor_keeps_engaged_zero_facing() {
    // PathMovementGenerator.h:66-67 and .cpp:29-33.
    let g = SourceFixedPathGenerator::from_points(
        &[[1., 2., 3.], [4., 5., 6.]],
        Some(0.),
        FixedPathOptions::default(),
    );
    assert_eq!(g.path[0].0, 0);
    assert_eq!(g.path[0].1.orientation, Some(0.));
    assert_eq!(g.path[1].1.orientation, Some(0.));
}
#[test]
fn movement_inform_uses_source_off_by_one_and_first_cycle_rule() {
    // PathMovementGenerator.cpp:198. The normal noncyclic first notification is the second map node.
    let g =
        SourceFixedPathGenerator::from_path(Some(&path(&[0, 0, 0])), FixedPathOptions::default());
    assert_eq!(g.inform_node().unwrap().0, 14);
    let mut p = g.progress();
    p.first_cycle = true;
    g.progress.set(p);
    assert_eq!(g.inform_node().unwrap().0, 7);
    p.index = 2;
    g.progress.set(p);
    assert_eq!(g.inform_node().unwrap().0, 21);
    p.index = 3;
    g.progress.set(p);
    assert_eq!(g.inform_node().unwrap().0, 7);
}
#[test]
fn empty_path_inform_refuses_modulo_zero_and_signed_index_overflow() {
    // PathMovementGenerator.cpp:198 evaluates int32 addition before size_t modulo.
    let g = SourceFixedPathGenerator::from_path(None, FixedPathOptions::default());
    assert_eq!(
        g.inform_node().unwrap_err().id,
        "execution.fixed_path.inform_modulo_zero_undefined"
    );
    let g = SourceFixedPathGenerator::from_path(
        Some(&path(&[0])),
        FixedPathOptions {
            offset: i32::MAX,
            ..Default::default()
        },
    );
    assert_eq!(
        g.inform_node().unwrap_err().id,
        "execution.fixed_path.index_addition_undefined"
    );
    let mut p = g.progress();
    p.first_cycle = true;
    g.progress.set(p);
    assert_eq!(g.inform_node().unwrap().0, 7);
}

fn setup() -> (SourceObjects, SourceUnits, SourceMotionMaster) {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(unit, 7, SourceGuid::with_entry(HighGuid::Creature, 901, 7))
        .unwrap();
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&super::super::super::motion_master_tests::template()),
        )
        .unwrap();
    (
        objects,
        units,
        SourceMotionMaster::new(
            unit,
            MotionDefault {
                movement_type: 3,
                default_path: 0,
            },
        ),
    )
}
#[derive(Default)]
struct Owner {
    finalized: bool,
    index: i32,
    destination: [f32; 3],
    log: Vec<String>,
    paths: Option<std::rc::Rc<SourceWaypointPaths>>,
    client: bool,
    pause_in_script: Option<i32>,
}
impl MotionExecutionOwner for Owner {
    fn waypoint_paths(&mut self) -> Result<std::rc::Rc<SourceWaypointPaths>> {
        self.paths
            .clone()
            .ok_or_else(|| refuse("execution.waypoint.loaded_waypoint_manager_owner"))
    }
    fn spline_finalized(&mut self, _: ObjectHandle) -> Result<bool> {
        self.log.push("finalized".into());
        Ok(self.finalized)
    }
    fn client_controlled(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.client)
    }
    fn fixed_path_spline(&mut self, _: ObjectHandle) -> Result<FixedPathSplineState> {
        Ok(FixedPathSplineState {
            destination: self.destination,
            finalized: self.finalized,
            index: self.index,
        })
    }
    fn motion_call(
        &mut self,
        _: &mut SourceMotionMaster,
        call: MotionCall,
        _: &mut MotionContext<'_>,
    ) -> Result<()> {
        self.log.push(format!("{call:?}"));
        Ok(())
    }
    fn fixed_path_script(
        &mut self,
        g: &SourceFixedPathGenerator,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        script: u32,
        target: Option<SourceGuid>,
    ) -> Result<()> {
        self.log.push(format!("script:{script}:{target:?}"));
        if let Some(delta) = self.pause_in_script {
            g.add_pause(delta, true);
        }
        Ok(())
    }
    fn fixed_path_inform(
        &mut self,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        kind: u32,
        point: u32,
    ) -> Result<()> {
        self.log.push(format!("inform:{kind}:{point}"));
        Ok(())
    }
}
#[test]
fn initialize_stops_previous_spline_before_preparing_and_refuses_before_launch() {
    // PathMovementGenerator.cpp:61-70,240-251,274.
    for (client, expected) in [(false, "InterruptMoving"), (true, "StopMovingForced")] {
        let (objects, mut units, mut master) = setup();
        let mut context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let g =
            SourceFixedPathGenerator::from_path(Some(&path(&[0, 0])), FixedPathOptions::default());
        let mut owner = Owner {
            client,
            ..Default::default()
        };
        assert_eq!(
            g.initialize(&mut master, &mut context, &mut owner)
                .unwrap_err()
                .id,
            "execution.fixed_path.f6_move_spline_init_owner"
        );
        assert_eq!(owner.log, ["finalized", expected]);
        assert_eq!(
            context.units.get(&objects, master.unit).unwrap().state
                & (WANDER_ACTIVE | WANDER_MOVING),
            WANDER_ACTIVE
        );
        assert!(!g.progress().first_cycle);
    }
}
#[test]
fn reset_only_rebuilds_abstract_spline_without_launching_or_setting_first_cycle() {
    // PathMovementGenerator.cpp:135-137,266-269, not Fixed::Initialize.
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let g = SourceFixedPathGenerator::from_path(Some(&path(&[0, 20])), FixedPathOptions::default());
    g.speed_changed();
    g.reset(
        &mut master,
        &mut context,
        &mut Owner {
            finalized: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(g.progress().wait, 20);
    assert!(!g.progress().speed_changed);
    assert!(!g.progress().first_cycle);
    assert_eq!(g.spline.borrow().len(), 3);
}
#[test]
fn empty_path_initializes_but_expires_on_update_and_finalize_stops_only_if_needed() {
    // PathMovementGenerator.cpp:72-75,181-193,254-258.
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let g = SourceFixedPathGenerator::from_path(None, FixedPathOptions::default());
    let mut owner = Owner {
        finalized: true,
        ..Default::default()
    };
    g.initialize(&mut master, &mut context, &mut owner).unwrap();
    assert!(!g.update(&mut master, 1, &mut context, &mut owner).unwrap());
    g.finalize(&mut master, &mut context, &mut owner).unwrap();
    assert_eq!(
        context.units.get(&objects, master.unit).unwrap().state & (WANDER_ACTIVE | WANDER_MOVING),
        0
    );
    assert_eq!(owner.log, ["finalized", "finalized"]);
}
#[test]
fn finalized_noncyclic_arrival_runs_script_before_ai_and_waits_before_next_segment() {
    // PathMovementGenerator.cpp:168-193,198-223. Authored spline-read state only.
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut nodes = path(&[10, 0]);
    nodes.get_mut(&7).unwrap().script = 88;
    let g = SourceFixedPathGenerator::from_path(Some(&nodes), FixedPathOptions::default());
    g.prepare_spline().unwrap();
    let mut p = g.progress();
    p.first_cycle = true;
    g.progress.set(p);
    let mut owner = Owner {
        finalized: true,
        index: 1,
        destination: [0., 2., 3.],
        ..Default::default()
    };
    assert!(g.update(&mut master, 9, &mut context, &mut owner).unwrap());
    assert_eq!(&owner.log[..2], &["script:88:None", "inform:3:7"]);
    assert_eq!((g.progress().index, g.progress().wait), (1, 1));
    assert_eq!(
        g.update(&mut master, 1, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.fixed_path.f6_move_spline_init_owner"
    );
}
#[test]
fn completed_path_returns_false_after_timer_and_destination_tampering_expires() {
    // PathMovementGenerator.cpp:145-150,188-193.
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let g = SourceFixedPathGenerator::from_path(Some(&path(&[0, 0])), FixedPathOptions::default());
    g.prepare_spline().unwrap();
    let mut owner = Owner {
        index: 2,
        finalized: true,
        destination: [1., 2., 3.],
        ..Default::default()
    };
    assert!(!g.update(&mut master, 0, &mut context, &mut owner).unwrap());
    assert!(owner.log.iter().any(|e| e == "inform:3:14"));
    let g = SourceFixedPathGenerator::from_path(Some(&path(&[0, 0])), FixedPathOptions::default());
    g.prepare_spline().unwrap();
    owner.finalized = false;
    owner.destination = [9.; 3];
    owner.log.clear();
    assert!(!g.update(&mut master, 0, &mut context, &mut owner).unwrap());
    assert!(owner.log.is_empty());
    owner.finalized = true;
    assert_eq!(
        g.update(&mut master, 0, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.fixed_path.f6_move_spline_init_owner"
    );
}
#[test]
fn cyclic_initial_zero_skips_inform_and_wraps_through_extra_spline_point() {
    // PathMovementGenerator.cpp:153-164 uses size()+1, not size().
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let g = SourceFixedPathGenerator::from_path(
        Some(&path(&[0, 0])),
        FixedPathOptions {
            cyclic: true,
            ..Default::default()
        },
    );
    g.prepare_spline().unwrap();
    let mut p = g.progress();
    p.first_cycle = true;
    g.progress.set(p);
    let mut owner = Owner {
        index: 2,
        destination: [1., 2., 3.],
        ..Default::default()
    };
    assert!(g.update(&mut master, 0, &mut context, &mut owner).unwrap());
    assert_eq!(
        owner
            .log
            .iter()
            .filter(|s| s.starts_with("inform"))
            .collect::<Vec<_>>(),
        [&"inform:3:14".to_owned()]
    );
    owner.index = 0;
    owner.log.clear();
    assert!(g.update(&mut master, 0, &mut context, &mut owner).unwrap());
    assert!(!g.progress().first_cycle);
    assert!(owner.log.iter().any(|s| s == "inform:3:7"));
}
#[test]
fn forced_script_pause_clears_spline_without_borrow_panic_and_uses_new_timer() {
    // ScriptsStart is synchronous, cpp:205 then timer Update :181.
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut nodes = path(&[0, 0]);
    nodes.get_mut(&14).unwrap().script = 88;
    let g = SourceFixedPathGenerator::from_path(Some(&nodes), FixedPathOptions::default());
    g.prepare_spline().unwrap();
    let mut owner = Owner {
        index: 1,
        finalized: true,
        destination: [1., 2., 3.],
        pause_in_script: Some(50),
        ..Default::default()
    };
    assert!(g.update(&mut master, 9, &mut context, &mut owner).unwrap());
    assert_eq!(g.progress().wait, 41);
    assert!(g.spline.borrow().is_empty());
}
#[test]
fn motion_master_default_pushes_fixed_before_first_spline_query() {
    // MotionMaster.cpp:79-81 and PathMovementGenerator.cpp:234-236.
    let (objects, mut units, mut master) = setup();
    let paths = SourceWaypointPaths::load(
        [
            vec![SourceWaypointRow {
                ordinal: 0,
                owner: 7,
                path: 0,
                point: 1,
                node: path(&[0])[&7],
            }],
            vec![],
            vec![],
        ],
        &std::collections::BTreeSet::from([7]),
        &std::collections::BTreeSet::from([901]),
        None,
    )
    .unwrap();
    let mut owner = Owner {
        paths: Some(std::rc::Rc::new(paths)),
        finalized: true,
        ..Default::default()
    };
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        master.initialize(&mut context, &mut owner).unwrap_err().id,
        "execution.fixed_path.f6_move_spline_init_owner"
    );
    assert_eq!((master.current_type(), master.len()), (3, 1));
    master
        .pause_waypoints(20, &mut context, &mut owner)
        .unwrap();
    master.unpause_waypoints(&mut context).unwrap();
}
#[test]
fn explicit_path_selection_and_point_entrypoint_obey_motionmaster_mutation_order() {
    // MotionMaster.cpp:425-444,599-620. Path ID is set before selection.
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master
        .move_path_points(
            &[],
            0.,
            FixedPathOptions::default(),
            &mut context,
            &mut Owner::default(),
        )
        .unwrap();
    assert!(master.is_empty());
    struct Missing;
    impl MotionExecutionOwner for Missing {}
    assert_eq!(
        master
            .move_path(
                -1,
                WaypointOrigin::WaypointPath,
                FixedPathOptions::default(),
                &mut context,
                &mut Missing
            )
            .unwrap_err()
            .id,
        "execution.waypoint.loaded_waypoint_manager_owner"
    );
    assert_eq!(master.path_id().unwrap(), u32::MAX);
    let mut owner = Owner {
        finalized: true,
        ..Default::default()
    };
    assert_eq!(
        master
            .move_path_points(
                &[[1.; 3]],
                0.,
                FixedPathOptions::default(),
                &mut context,
                &mut owner
            )
            .unwrap_err()
            .id,
        "execution.fixed_path.f6_move_spline_init_owner"
    );
    // Mutate initializes before pushing, unlike default construction.
    assert!(master.is_empty());
}
