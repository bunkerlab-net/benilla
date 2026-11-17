use super::*;
use std::collections::BTreeSet;

fn row(owner: u32, path: u32, point: u32, delay: u32, script: u32) -> SourceWaypointRow {
    SourceWaypointRow {
        ordinal: u64::from(point),
        owner,
        path,
        point,
        node: SourceWaypointNode {
            position: [1., 2., 3.],
            orientation: None,
            delay,
            script,
        },
    }
}
fn paths(rows: [Vec<SourceWaypointRow>; 3]) -> SourceWaypointPaths {
    SourceWaypointPaths::load(
        rows,
        &BTreeSet::from([7]),
        &BTreeSet::from([901]),
        Some(&WaypointScriptMaps::default()),
    )
    .unwrap()
}
#[test]
fn default_path_only_dereferences_null_origin_for_waypoint_fallback() {
    let p = paths([
        vec![row(7, 0, 1, 0, 0)],
        vec![row(901, 0, 2, 0, 0)],
        vec![row(901 << 8, 0, 3, 0, 0)],
    ]);
    assert!(p
        .default_path(901, 7, None)
        .unwrap()
        .unwrap()
        .contains_key(&1));
    assert!(p
        .default_path(901, 8, None)
        .unwrap()
        .unwrap()
        .contains_key(&2));
    let p = paths([vec![], vec![], vec![row(901 << 8, 0, 3, 0, 0)]]);
    assert_eq!(
        p.default_path(901, 8, None).unwrap_err().id,
        "execution.waypoint.GetDefaultPath_null_origin_undefined"
    );
    let mut origin = WaypointOrigin::Guid;
    assert!(p.default_path(901, 8, Some(&mut origin)).unwrap().is_some());
    assert_eq!(origin, WaypointOrigin::WaypointPath);
    assert!(p.default_path(902, 8, Some(&mut origin)).unwrap().is_none());
    assert_eq!(origin, WaypointOrigin::WaypointPath);
    assert!(p
        .path_from_origin(901, 8, 0, WaypointOrigin::None)
        .unwrap()
        .is_none());
    assert!(p
        .path_from_origin(901, 8, 901 << 8, WaypointOrigin::WaypointPath)
        .unwrap()
        .is_some());
}

#[test]
fn loaded_manager_reaches_spell_query_after_initializing_the_path_cursor() {
    struct LoadedManager(Rc<SourceWaypointPaths>);
    impl MotionExecutionOwner for LoadedManager {
        fn waypoint_paths(&mut self) -> Result<Rc<SourceWaypointPaths>> {
            Ok(self.0.clone())
        }
    }
    let (_, p) = loaded(&[(1, 0, 0), (3, 0, 0)]);
    let g = SourceWaypointGenerator::default();
    let (objects, mut units, mut master) = setup();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        g.initialize_path(
            &mut master,
            &mut context,
            &mut LoadedManager(p),
            WaypointPathRequest::default()
        )
        .unwrap_err()
        .id,
        "execution.waypoint.f5_spell_movement_cast_query_owner"
    );
    assert_eq!(g.progress().origin, WaypointOrigin::Guid);
    assert_eq!(g.progress().current, 1);
    assert_eq!(g.progress().last_reached, 0);
    assert_eq!(g.progress().wait, 0);
    assert_eq!(g.node_after(1, false).unwrap(), Some(3));
}
fn loaded(points: &[(u32, u32, u32)]) -> (SourceWaypointGenerator, Rc<SourceWaypointPaths>) {
    let paths = Rc::new(paths([
        points.iter().map(|&(p, d, s)| row(7, 0, p, d, s)).collect(),
        vec![],
        vec![],
    ]));
    let g = SourceWaypointGenerator::default();
    g.load_path(&paths, 901, 7, 0, WaypointOrigin::None)
        .unwrap();
    (g, paths)
}
fn setup() -> (SourceObjects, SourceUnits, SourceMotionMaster) {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects
        .set_pose(unit, SourcePose([1f32.to_bits(); 4]))
        .unwrap();
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
    let master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 2,
            default_path: 0,
        },
    );
    (objects, units, master)
}
#[derive(Default)]
struct Owner {
    paths: Option<Rc<SourceWaypointPaths>>,
    spline: Option<bool>,
    log: Vec<String>,
    scripts: bool,
    ai: bool,
    delta: i32,
    ai_path: u32,
    formation: Option<u32>,
    geometry: Option<SourceTerrainManager>,
    speed: Option<f32>,
    speed_reads: usize,
    passengers: Option<SourcePassengers>,
    path_starts: Vec<[f32; 3]>,
}
impl MotionExecutionOwner for Owner {
    fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
        self.geometry
            .as_mut()
            .ok_or_else(|| refuse("execution.pathfinder.live_geometry_owner"))
    }
    fn waypoint_speed(&mut self, _: ObjectHandle, _: &MotionContext<'_>) -> Result<f32> {
        self.speed_reads += 1;
        self.speed
            .ok_or_else(|| refuse("execution.waypoint.movement_speed_owner_261"))
    }
    fn pathfinder_passengers(&self) -> Result<&SourcePassengers> {
        self.passengers
            .as_ref()
            .ok_or_else(|| refuse("execution.pathfinder.transport_owner"))
    }
    fn calculate_pathfinder(
        &mut self,
        path: &mut SourcePathFinder,
        context: &MotionContext<'_>,
        start: [f32; 3],
        end: [f32; 3],
    ) -> Result<bool> {
        self.path_starts.push(start);
        let attachment = self
            .pathfinder_passengers()
            .and_then(|passengers| passengers.movement(path.unit).map(|state| state.transport))
            .map_err(|_| refuse("execution.pathfinder.transport_owner"));
        path.calculate(
            self.pathfinder_geometry()?,
            context,
            attachment,
            start,
            end,
            true,
            false,
        )
    }
    fn waypoint_paths(&mut self) -> Result<Rc<SourceWaypointPaths>> {
        self.paths
            .clone()
            .ok_or_else(|| refuse("execution.waypoint.loaded_waypoint_manager_owner"))
    }
    fn waypoint_formation_path(&mut self, _: ObjectHandle) -> Result<Option<u32>> {
        Ok(self.formation)
    }
    fn waypoint_casting_for_movement(
        &mut self,
        _: ObjectHandle,
        _: &MotionContext<'_>,
    ) -> Result<bool> {
        Ok(false)
    }
    fn spline_finalized(&mut self, _: ObjectHandle, _: &MotionContext<'_>) -> Result<bool> {
        self.log.push("spline".into());
        self.spline
            .ok_or_else(|| refuse("execution.motion.f6_spline_finalized_owner"))
    }
    fn waypoint_position(&mut self, _: ObjectHandle, _: &MotionContext<'_>) -> Result<[f32; 4]> {
        self.log.push("position".into());
        Ok([5., 6., 7., 8.])
    }
    fn waypoint_ai_path(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(self.ai_path)
    }
    fn waypoint_script(
        &mut self,
        g: &SourceWaypointGenerator,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        id: u32,
        _: Option<SourceGuid>,
    ) -> Result<()> {
        self.log.push(format!("script:{id}"));
        if !self.scripts {
            return Err(refuse(
                "execution.waypoint.f5_dbscript_movement_execution_owner",
            ));
        }
        g.add_pause(self.delta, false);
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
        self.log.push(format!("inform:{kind}:{point}"));
        if !self.ai {
            return Err(refuse("execution.waypoint.f5_ai_movement_inform_owner"));
        }
        Ok(())
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
}
#[test]
fn default_path_precedence_guid_then_entry_then_shifted_waypoint_key() {
    // WaypointManager.h:69-91,169-178. Third lookup is entry<<8, not entry.
    let rows = [
        vec![row(7, 0, 3, 0, 0)],
        vec![row(901, 0, 5, 0, 0)],
        vec![row(901 << 8, 0, 9, 0, 0), row(901, 0, 8, 0, 0)],
    ];
    let p = paths(rows.clone());
    let (origin, path) = p.select(901, 7, 0, WaypointOrigin::None).unwrap();
    assert_eq!(origin, WaypointOrigin::Guid);
    assert!(path.unwrap().contains_key(&3));
    let p = paths([vec![], rows[1].clone(), rows[2].clone()]);
    assert_eq!(
        p.select(901, 7, 0, WaypointOrigin::None).unwrap().0,
        WaypointOrigin::Entry
    );
    let p = paths([vec![], vec![], rows[2].clone()]);
    let (origin, path) = p.select(901, 7, 0, WaypointOrigin::None).unwrap();
    assert_eq!(origin, WaypointOrigin::WaypointPath);
    assert!(path.unwrap().contains_key(&9));
}
#[test]
fn explicit_origins_do_not_fall_back_and_path_id_limits_are_source_defined() {
    let mut p = paths([
        vec![row(7, 0, 1, 0, 0)],
        vec![row(901, 254, 2, 0, 0)],
        vec![row(u32::MAX, 0, 3, 0, 0)],
    ]);
    p.load_external([row(901, 254, 4, 0, 0)]);
    for origin in [WaypointOrigin::Entry, WaypointOrigin::External] {
        assert!(p.select(901, 7, 254, origin).unwrap().1.is_some());
        for invalid in [-1, 255] {
            assert!(p.select(901, 7, invalid, origin).unwrap().1.is_none());
        }
    }
    assert!(p
        .select(901, 7, 254, WaypointOrigin::None)
        .unwrap()
        .1
        .unwrap()
        .contains_key(&2));
    assert!(p
        .select(901, 7, -1, WaypointOrigin::WaypointPath)
        .unwrap()
        .1
        .unwrap()
        .contains_key(&3));
    assert!(p
        .select(901, 7, -1, WaypointOrigin::Guid)
        .unwrap()
        .1
        .unwrap()
        .contains_key(&1));
    assert!(p
        .select(901, 7, 0, WaypointOrigin::Entry)
        .unwrap()
        .1
        .is_none());
}
#[test]
fn external_paths_require_registration_owner_and_scriptdev_blacklists_point_zero() {
    let mut p = paths([vec![], vec![], vec![]]);
    assert_eq!(
        p.select(901, 7, 0, WaypointOrigin::External)
            .unwrap_err()
            .id,
        "execution.waypoint.scriptdev_external_paths_owner"
    );
    p.load_scriptdev(
        vec![
            row(901, 0, 1, 0, 0),
            row(901, 0, 0, 0, 0),
            row(901, 1, 2, 0, 0),
            row(999, 0, 1, 0, 0),
        ],
        &BTreeSet::from([901]),
    );
    assert!(p
        .select(901, 7, 0, WaypointOrigin::External)
        .unwrap()
        .1
        .is_none());
    assert!(p
        .select(901, 7, 1, WaypointOrigin::External)
        .unwrap()
        .1
        .unwrap()
        .contains_key(&2));
    assert!(p
        .select(999, 7, 0, WaypointOrigin::External)
        .unwrap()
        .1
        .is_none());
}
#[test]
fn manager_orders_nodes_and_overwrites_duplicate_points_in_cursor_order() {
    let mut a = row(7, 0, 2, 1, 0);
    let b = row(7, 0, 2, 9, 0);
    a.ordinal = 8;
    let p = paths([
        vec![row(7, 0, 5, 0, 0), a, row(7, 0, 1, 0, 0), b],
        vec![],
        vec![],
    ]);
    let path = p
        .select(901, 7, 0, WaypointOrigin::None)
        .unwrap()
        .1
        .unwrap();
    assert_eq!(path.keys().copied().collect::<Vec<_>>(), [1, 2, 5]);
    assert_eq!(path[&2].delay, 9);
}
#[test]
fn manager_skips_missing_owners_and_retains_waypoint_wrong_map_erasure_quirk() {
    // cpp:215-227,345-354,454-498. waypoint point 0 erases ENTRY map.
    let p = paths([
        vec![row(8, 0, 1, 0, 0)],
        vec![row(901, 0, 1, 0, 0), row(999, 0, 1, 0, 0)],
        vec![row(901 << 8, 0, 0, 0, 0)],
    ]);
    assert!(p
        .select(901, 8, 0, WaypointOrigin::Guid)
        .unwrap()
        .1
        .is_none());
    assert!(p
        .select(999, 8, 0, WaypointOrigin::Entry)
        .unwrap()
        .1
        .is_none());
    let (origin, path) = p.select(901, 8, 0, WaypointOrigin::None).unwrap();
    assert_eq!(origin, WaypointOrigin::WaypointPath);
    assert!(path.unwrap().contains_key(&0));
    let p = paths([
        vec![row(7, 0, 1, 0, 0), row(7, 0, 0, 0, 0)],
        vec![row(901, 0, 1, 0, 0), row(901, 0, 0, 0, 0)],
        vec![],
    ]);
    assert!(p
        .select(901, 7, 0, WaypointOrigin::None)
        .unwrap()
        .1
        .is_none());
}
#[test]
fn loader_requires_loaded_scriptmgr_maps_only_at_nonzero_script() {
    let sets = BTreeSet::from([7]);
    let templates = BTreeSet::from([901]);
    assert!(SourceWaypointPaths::load(
        [vec![row(7, 0, 1, 0, 0)], vec![], vec![]],
        &sets,
        &templates,
        None
    )
    .is_ok());
    let err = SourceWaypointPaths::load(
        [vec![row(7, 0, 1, 10, 4)], vec![], vec![]],
        &sets,
        &templates,
        None,
    )
    .err()
    .unwrap();
    assert_eq!(
        err.id,
        "execution.waypoint.scriptmgr_dbscript_loaded_maps_owner"
    );
}
#[test]
fn check_dbscript_despawn_movement_run_and_delayed_boundaries() {
    for (command, despawn_delay, script_delay, expected) in [
        (18, 0, 0, 1000),
        (18, 1, 0, 0),
        (20, 1, 0, 1000),
        (25, 1, 0, 1000),
        (20, 0, 1, 0),
        (1, 0, 0, 0),
    ] {
        let mut maps = WaypointScriptMaps::default();
        maps.movement.insert(
            4,
            vec![WaypointScriptStep {
                command,
                despawn_delay,
                delay: script_delay,
                ..Default::default()
            }],
        );
        let mut n = row(7, 0, 1, 0, 4).node;
        maps.check(&mut n).unwrap();
        assert_eq!(n.delay, expected);
        n.delay = 15;
        maps.check(&mut n).unwrap();
        assert_eq!(n.delay, 15);
    }
    let mut n = row(7, 0, 1, 0, 99).node;
    WaypointScriptMaps::default().check(&mut n).unwrap();
    assert_eq!(n.script, 99);
}
#[test]
fn check_dbscript_stops_at_first_delayed_step_and_relay_outer_reference_is_noop() {
    let mut maps = WaypointScriptMaps::default();
    maps.movement.insert(
        4,
        vec![
            WaypointScriptStep {
                delay: 1,
                ..Default::default()
            },
            WaypointScriptStep {
                command: 20,
                ..Default::default()
            },
        ],
    );
    let mut n = row(7, 0, 1, 0, 4).node;
    maps.check(&mut n).unwrap();
    assert_eq!(n.delay, 0);
    maps.movement.insert(
        4,
        vec![WaypointScriptStep {
            command: 45,
            relay: 9,
            relay_template: 6,
            ..Default::default()
        }],
    );
    maps.check(&mut n).unwrap();
    assert!(maps.relay_templates.borrow()[&6].is_empty());
    maps.relay_templates.borrow_mut().insert(6, vec![9]);
    maps.check(&mut n).unwrap();
    assert_eq!(n.delay, 0);
    maps.relay.insert(9);
    maps.relay_templates.borrow_mut().clear();
    maps.check(&mut n).unwrap();
    assert_eq!(n.delay, 0);
    assert!(maps.relay_templates.borrow().is_empty());
}
#[test]
fn coordinate_repair_refuses_at_both_sides_of_finite_and_map_boundaries() {
    for x in [17066., 17067., f32::NAN] {
        let mut r = row(7, 0, 1, 0, 0);
        r.node.position[0] = x;
        let result = SourceWaypointPaths::load(
            [vec![r], vec![], vec![]],
            &BTreeSet::from([7]),
            &BTreeSet::new(),
            None,
        );
        assert_eq!(result.is_ok(), x == 17066.);
        if let Err(e) = result {
            assert_eq!(e.id, "execution.waypoint.f6_coordinate_repair_owner");
        }
    }
}
#[test]
fn default_constructor_initializes_before_loading_and_stops_at_first_f6_query() {
    let (_, p) = loaded(&[(1, 0, 50), (3, 1, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = WAYPOINT_PAUSED;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        paths: Some(p),
        ..Default::default()
    };
    assert_eq!(
        master.initialize(&mut c, &mut owner).unwrap_err().id,
        "execution.motion.f6_spline_finalized_owner"
    );
    assert_eq!(master.current_type(), 2);
    assert_eq!(master.len(), 1);
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        WANDER_ACTIVE | WANDER_MOVING
    );
    assert_eq!(owner.log, ["spline"]);
    assert_eq!(master.reset_position(), Some([0.; 4]));
}
#[test]
fn initial_delay_does_not_defer_first_path_send_and_single_node_delay_stays_zero() {
    // cpp:70-82 copies modNode; cpp:95,102,130-131 still sends immediately.
    let (g, p) = loaded(&[(1, 0, 0)]);
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().map = Some(MapIdentity {
        map: 0,
        instance: 7,
        creation_generation: 1,
    });
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let Some(geometry) = pathfinder::tests::geometry(0, 0) else {
        return;
    };
    let mut owner = Owner {
        paths: Some(p.clone()),
        spline: Some(true),
        geometry: Some(geometry),
        ..Default::default()
    };
    assert_eq!(
        g.initialize_path(
            &mut master,
            &mut c,
            &mut owner,
            WaypointPathRequest {
                delay: 500,
                ..Default::default()
            }
        )
        .unwrap_err()
        .id,
        "execution.waypoint.movement_speed_owner_261"
    );
    assert_eq!(
        owner
            .geometry
            .as_ref()
            .unwrap()
            .pathfinder_constructor_completions,
        0
    );
    assert_eq!(owner.speed_reads, 1);
    assert_eq!(g.progress().wait, 500);
    assert_eq!(
        p.select(901, 7, 0, WaypointOrigin::None)
            .unwrap()
            .1
            .unwrap()[&1]
            .delay,
        0
    );
}
#[test]
fn missing_path_is_defined_idle_but_present_empty_path_refuses_iterator_undefined() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let g = SourceWaypointGenerator::default();
    let mut owner = Owner {
        paths: Some(Rc::new(paths([vec![], vec![], vec![]]))),
        ..Default::default()
    };
    g.initialize_path(
        &mut master,
        &mut c,
        &mut owner,
        WaypointPathRequest::default(),
    )
    .unwrap();
    assert!(g.update(&mut master, 10, &mut c, &mut owner).unwrap());
    assert!(owner.log.is_empty());
    assert_eq!(
        g.send_next(&mut master, &mut c, &mut owner).unwrap_err().id,
        "execution.waypoint.null_path_iterator_undefined"
    );
    let mut empty = paths([vec![], vec![], vec![]]);
    empty.insert_empty_guid(7);
    owner.paths = Some(Rc::new(empty));
    assert_eq!(
        g.initialize_path(
            &mut master,
            &mut c,
            &mut owner,
            WaypointPathRequest::default()
        )
        .unwrap_err()
        .id,
        "execution.waypoint.empty_path_iterator_undefined"
    );
    assert!(g.update(&mut master, 1, &mut c, &mut owner).unwrap());
}
#[test]
fn uninitialized_path_read_refuses_but_initialize_alone_never_reads_it() {
    let g = SourceWaypointGenerator::default();
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner::default();
    g.initialize(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(
        g.reset(&mut master, &mut c, &mut owner).unwrap_err().id,
        "execution.waypoint.path_uninitialized_undefined"
    );
    g.load_path(
        &paths([vec![], vec![], vec![]]),
        901,
        7,
        0,
        WaypointOrigin::None,
    )
    .unwrap();
    g.reset(&mut master, &mut c, &mut owner).unwrap();
}
#[test]
fn initialize_selects_uint_max_startpoint_and_prior_map_key() {
    let (g, p) = loaded(&[(2, 0, 0), (8, 0, 0), (u32::MAX, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        paths: Some(p),
        ..Default::default()
    };
    assert_eq!(
        g.initialize_path(
            &mut master,
            &mut c,
            &mut owner,
            WaypointPathRequest::default()
        )
        .unwrap_err()
        .id,
        "execution.motion.f6_spline_finalized_owner"
    );
    assert_eq!(g.progress().current, u32::MAX);
    assert_eq!(g.progress().last_reached, 8);
}
#[test]
fn arrival_kernel_orders_script_ai_position_delay_and_repeats_last_node() {
    // Kernel-only arrival state, not evidence that a spline was launched.
    let (g, _) = loaded(&[(2, 200, 4), (8, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().has_ai = true;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        scripts: true,
        ai: true,
        delta: 50,
        spline: Some(true),
        ..Default::default()
    };
    g.arrived(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(owner.log, ["script:4", "inform:2:2", "position", "spline"]);
    assert_eq!(g.progress().wait, 250);
    assert_eq!(g.progress().current, 8);
    assert_eq!(g.progress().last_reached, 2);
    g.stop(0);
    owner.log.clear();
    g.arrived(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(g.progress().current, 2);
    assert_eq!(g.progress().last_reached, 8);
    assert_eq!(owner.log, ["inform:2:8", "position"]);
}
#[test]
fn missing_script_execution_stops_before_ai_and_node_switch() {
    let (g, _) = loaded(&[(1, 0, 4), (2, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().has_ai = true;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner::default();
    assert_eq!(
        g.arrived(&mut master, &mut c, &mut owner).unwrap_err().id,
        "execution.waypoint.f5_dbscript_movement_execution_owner"
    );
    assert_eq!(owner.log, ["script:4"]);
    assert_eq!(g.progress().last_reached, 1);
    assert_eq!(g.progress().current, 1);
}
#[test]
fn missing_ai_owner_stops_after_script_before_position_or_delay() {
    let (g, _) = loaded(&[(1, 10, 4), (2, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().has_ai = true;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        scripts: true,
        ..Default::default()
    };
    assert_eq!(
        g.arrived(&mut master, &mut c, &mut owner).unwrap_err().id,
        "execution.waypoint.f5_ai_movement_inform_owner"
    );
    assert_eq!(owner.log, ["script:4", "inform:2:1"]);
    assert_eq!(g.progress().wait, 0);
}
#[test]
fn script_cancelled_delay_restarts_before_current_node_is_assigned() {
    let (g, _) = loaded(&[(1, 10, 4), (2, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        scripts: true,
        delta: -10,
        ..Default::default()
    };
    assert_eq!(
        g.arrived(&mut master, &mut c, &mut owner).unwrap_err().id,
        "execution.motion.f6_spline_finalized_owner"
    );
    assert_eq!(owner.log, ["script:4", "position", "spline"]);
    assert_eq!(g.cursor.get(), Some(2));
    assert_eq!(g.progress().current, 1);
}
#[test]
fn script_added_delay_stops_active_spline_after_script_and_position() {
    let (g, _) = loaded(&[(1, 0, 4), (2, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        scripts: true,
        delta: 5,
        spline: Some(false),
        ..Default::default()
    };
    g.arrived(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(owner.log, ["script:4", "position", "spline", "StopSpline"]);
    assert_eq!(g.progress().wait, 5);
}
#[test]
fn pause_arithmetic_wraps_unsigned_before_signed_clamp_and_force_preserves_negative_bits() {
    let g = SourceWaypointGenerator::default();
    for (initial, delta, expected) in [
        (10, -9, 1),
        (10, -10, 1),
        (10, -11, 1),
        (1, i32::MAX, 1),
        (u32::MAX, 2, 1),
        (u32::MAX, 1, 1),
        (i32::MAX as u32, -1, 2147483646),
    ] {
        g.stop(initial);
        g.add_pause(delta, false);
        assert_eq!(g.progress().wait, expected);
    }
    g.add_pause(-1, true);
    assert_eq!(g.progress().wait, u32::MAX);
    g.stop(0);
    g.add_pause(-50, false);
    assert_eq!(g.progress().wait, 0);
    assert_eq!(g.progress().script_time, -50);
}
#[test]
fn exact_timer_boundaries_and_paused_minimum_one() {
    let g = SourceWaypointGenerator::default();
    g.stop(10);
    assert!(!g.can_move(9, 0));
    assert_eq!(g.progress().wait, 1);
    assert!(g.can_move(1, 0));
    assert_eq!(g.progress().wait, 0);
    g.stop(10);
    assert!(!g.can_move(10, WAYPOINT_PAUSED));
    assert_eq!(g.progress().wait, 1);
    assert!(!g.can_move(u32::MAX, WAYPOINT_PAUSED));
    assert_eq!(g.progress().wait, 1);
    assert!(g.can_move(1, 0));
}
#[test]
fn set_next_waypoint_uses_size_modulo_not_largest_key_and_resets_wait() {
    let (g, _) = loaded(&[(1, 0, 0), (2, 0, 0), (8, 0, 0)]);
    assert!(g.set_next_waypoint(8).unwrap());
    assert_eq!(g.progress().current, 2);
    assert_eq!(g.progress().last_reached, 1);
    assert_eq!(g.progress().wait, 1);
    assert!(!g.set_next_waypoint(9).unwrap());
    assert_eq!(g.progress().current, 2);
    let empty = SourceWaypointGenerator::default();
    empty
        .load_path(
            &paths([vec![], vec![], vec![]]),
            901,
            7,
            0,
            WaypointOrigin::None,
        )
        .unwrap();
    assert!(!empty.set_next_waypoint(1).unwrap());
}
#[test]
fn interrupt_saves_position_before_interrupt_then_clears_state_and_restores_walk() {
    let (g, _) = loaded(&[(1, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state =
        WANDER_ACTIVE | WANDER_MOVING | WAYPOINT_PAUSED;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner::default();
    g.interrupt(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(
        owner.log,
        ["position", "InterruptMoving", "RestoreWalk(true)"]
    );
    assert_eq!(g.reset_position(), Some([5., 6., 7., 8.]));
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        WAYPOINT_PAUSED
    );
    owner.log.clear();
    g.interrupt(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(owner.log, ["InterruptMoving", "RestoreWalk(true)"]);
    g.reset(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        WAYPOINT_PAUSED | WANDER_ACTIVE
    );
}
#[test]
fn finalize_clears_roaming_before_walk_refusal_and_reset_does_not_clear_timer() {
    struct Unbound;
    impl MotionExecutionOwner for Unbound {}
    let (g, _) = loaded(&[(1, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state =
        WANDER_ACTIVE | WANDER_MOVING | 0x0400_0000;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        g.finalize(&mut master, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.walk_movement_info_output_owner"
    );
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        0x0400_0000
    );
    g.stop(80);
    g.reset(&mut master, &mut c, &mut Unbound).unwrap();
    assert_eq!(g.progress().wait, 80);
    g.stop(0);
    assert_eq!(
        g.reset(&mut master, &mut c, &mut Unbound).unwrap_err().id,
        "execution.waypoint.f5_spell_movement_cast_query_owner"
    );
}
#[test]
fn waypoint_path_null_ai_guard_only_applies_to_continuing_indexed_arrival() {
    let (g, _) = loaded(&[(1, 0, 0), (2, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    let mut p = g.progress();
    p.origin = WaypointOrigin::WaypointPath;
    g.progress.set(p);
    g.node_indexes.borrow_mut().push_back(1); // Kernel-only post-launch state.
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner::default();
    assert_eq!(
        g.arrived(&mut master, &mut c, &mut owner).unwrap_err().id,
        "execution.waypoint.null_ai_path_query_undefined"
    );
    let (g, _) = loaded(&[(1, 0, 0), (2, 0, 0)]);
    let mut p = g.progress();
    p.origin = WaypointOrigin::WaypointPath;
    g.progress.set(p);
    g.node_indexes.borrow_mut().push_back(1);
    c.units.get_mut(&objects, master.unit).unwrap().has_ai = true;
    owner.ai = true;
    g.arrived(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(g.progress().current, 2);
}

#[test]
fn nonzero_default_path_and_overwrite_entry_select_the_explicit_entry_path() {
    // LoadPath cpp:39-49 does not consult GUID when pathId is nonzero.
    let p = Rc::new(paths([
        vec![row(7, 0, 1, 0, 0)],
        vec![row(901, 2, 8, 0, 0)],
        vec![],
    ]));
    let g = SourceWaypointGenerator::default();
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().entry = 902;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        paths: Some(p),
        ..Default::default()
    };
    let request = WaypointPathRequest {
        path_id: 2,
        overwrite_entry: 901,
        ..Default::default()
    };
    assert_eq!(
        g.initialize_path(&mut master, &mut c, &mut owner, request)
            .unwrap_err()
            .id,
        "execution.motion.f6_spline_finalized_owner"
    );
    assert_eq!(g.progress().origin, WaypointOrigin::Entry);
    assert_eq!(g.progress().current, 8);
}

#[test]
fn formation_leader_path_overrides_the_default_and_uses_waypoint_path_origin() {
    // MotionMaster.cpp:85-94, not the GUID/entry default lookup.
    let p = Rc::new(paths([
        vec![row(7, 0, 1, 0, 0)],
        vec![row(901, 2, 8, 0, 0)],
        vec![row(77, 0, 9, 0, 0)],
    ]));
    let (objects, mut units, mut master) = setup();
    master.default.default_path = 2;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        paths: Some(p),
        formation: Some(77),
        ..Default::default()
    };
    assert_eq!(
        master.initialize(&mut c, &mut owner).unwrap_err().id,
        "execution.motion.f6_spline_finalized_owner"
    );
    assert_eq!(master.path_id().unwrap(), 77);
    assert_eq!(master.current_type(), 2);
    assert_eq!(owner.log, ["spline"]);
}

#[test]
fn formation_and_default_origins_reach_path_initialization_for_both_generators() {
    // MotionMaster.cpp:83-94; cpp:44-49 selects only the requested origin.
    for movement in [2, 4] {
        for formation in [None, Some(77)] {
            let p = Rc::new(paths([
                vec![],
                if formation.is_none() {
                    vec![row(901, 2, 8, 0, 0)]
                } else {
                    vec![]
                },
                if formation.is_some() {
                    vec![row(77, 0, 9, 0, 0)]
                } else {
                    vec![]
                },
            ]));
            let (objects, mut units, mut master) = setup();
            master.default = MotionDefault {
                movement_type: movement,
                default_path: 2,
            };
            let mut c = MotionContext {
                objects: &objects,
                units: &mut units,
            };
            let mut owner = Owner {
                paths: Some(p),
                formation,
                ..Default::default()
            };
            assert_eq!(
                master.initialize(&mut c, &mut owner).unwrap_err().id,
                "execution.motion.f6_spline_finalized_owner"
            );
            assert_eq!(master.path_id().unwrap(), formation.unwrap_or(2));
            assert_eq!(master.current_type(), 2);
            assert_eq!(owner.log, ["spline"]);
            assert_eq!(
                c.units.get(&objects, master.unit).unwrap().state & WANDER_ACTIVE,
                WANDER_ACTIVE
            );
        }
    }
}

#[test]
fn arrival_delay_sum_wraps_and_signed_negative_does_not_start_a_wait() {
    // Kernel-only cpp:211 arithmetic at the two uint32-to-int32 boundaries.
    for (delay, delta, expected) in [
        (u32::MAX, 2, 1),
        (i32::MAX as u32, 1, 0),
        (i32::MAX as u32, 0, i32::MAX as u32),
    ] {
        let (g, _) = loaded(&[(1, delay, 4), (2, 0, 0)]);
        let (objects, mut units, mut master) = setup();
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let mut owner = Owner {
            scripts: true,
            delta,
            spline: Some(true),
            ..Default::default()
        };
        g.arrived(&mut master, &mut c, &mut owner).unwrap();
        assert_eq!(g.progress().wait, expected);
        assert_eq!(g.progress().current, 2);
    }
}

#[test]
fn not_move_preserves_wait_and_stopped_unit_starts_the_source_one_second_wait() {
    let (g, _) = loaded(&[(1, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = WANDER_ACTIVE | WANDER_MOVING | 0x10;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner::default();
    g.stop(50);
    assert!(g.update(&mut master, 100, &mut c, &mut owner).unwrap());
    assert_eq!(g.progress().wait, 50);
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        WANDER_ACTIVE | 0x10
    );
    assert!(owner.log.is_empty());
    c.units.get_mut(&objects, master.unit).unwrap().state = WANDER_ACTIVE;
    g.stop(0);
    g.update(&mut master, 100, &mut c, &mut owner).unwrap();
    assert_eq!(g.progress().wait, 1000);
}

#[test]
fn external_start_and_finished_callbacks_precede_f6_and_can_end_the_send() {
    let mut p = paths([vec![], vec![], vec![]]);
    p.load_external([row(901, 1, 1, 0, 0), row(901, 1, 2, 0, 0)]);
    let g = SourceWaypointGenerator::default();
    g.load_path(&p, 901, 7, 1, WaypointOrigin::External)
        .unwrap();
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().has_ai = true;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Owner {
        ai: true,
        ..Default::default()
    };
    // Object is not in world. Source cpp:346-347 returns after the callback.
    g.send_next(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(owner.log, ["inform:19:1"]);
    let mut progress = g.progress();
    progress.last_reached = 2;
    g.progress.set(progress);
    owner.log.clear();
    g.send_next(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(owner.log, ["inform:20:1"]);
}

#[test]
fn interrupt_captures_position_then_clears_roaming_before_missing_walk_owner() {
    struct MissingInterrupt;
    impl MotionExecutionOwner for MissingInterrupt {
        fn waypoint_position(
            &mut self,
            _: ObjectHandle,
            _: &MotionContext<'_>,
        ) -> Result<[f32; 4]> {
            Ok([9.; 4])
        }
    }
    let (g, _) = loaded(&[(1, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = WANDER_ACTIVE | WANDER_MOVING;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        g.interrupt(&mut master, &mut c, &mut MissingInterrupt)
            .unwrap_err()
            .id,
        "execution.motion.walk_movement_info_output_owner"
    );
    assert_eq!(c.units.get(&objects, master.unit).unwrap().state, 0);
    assert_eq!(g.reset_position(), Some([9.; 4]));
    assert!(c.spline_finalized(master.unit).unwrap());
    assert!(!c.spline(master.unit).unwrap().initialized());
    assert!(c
        .units
        .get(&objects, master.unit)
        .unwrap()
        .spline_outbound
        .is_empty());
}

#[test]
fn missing_default_path_preserves_the_previous_origin_and_node() {
    // GetDefaultPath writes origin only on a hit, WaypointManager.h:69-91.
    let (g, p) = loaded(&[(8, 0, 0)]);
    g.load_path(&p, 902, 9, 0, WaypointOrigin::None).unwrap();
    assert!(g.path().unwrap().is_none());
    assert_eq!(g.progress().origin, WaypointOrigin::Guid);
    assert_eq!(g.progress().current, 8);
}

#[test]
fn bound_speed_and_passengers_calculate_before_lookahead_and_spline_init() {
    if !cfg!(target_arch = "x86_64") {
        assert!(matches!(
            benilla_native_nav::live::MapMesh::new(include_bytes!(
                "../../../../../benilla-native-nav/tools/fixtures/authored.mmap"
            )),
            Err(benilla_native_nav::live::LiveError::NumericBinding)
        ));
        return;
    }
    let (g, p) = loaded(&[(1, 0, 0)]);
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().map = Some(MapIdentity {
        map: 0,
        instance: 7,
        creation_generation: 1,
    });
    let Some(mut geometry) = pathfinder::tests::geometry(0, 0) else {
        return;
    };
    let config=b"mmap.enabled = 1\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\nPathFinder.NormalizeZ = 0\n";
    geometry
        .mmap
        .initialize_recorded_world(config, &Sha256Id::of(config))
        .unwrap();
    let mut passengers = SourcePassengers::default();
    passengers
        .bind_unit(
            master.unit,
            PassengerMovement {
                transport: None,
                guid: 0,
                time: 0,
                offset: SourcePose([0; 4]),
                on_transport: false,
                movement_pose: objects.pose(master.unit).unwrap(),
                ghost: false,
                pet: None,
                mini_pet: None,
            },
            &objects,
        )
        .unwrap();
    let mut owner = Owner {
        paths: Some(p),
        spline: Some(true),
        geometry: Some(geometry),
        speed: Some(2.5),
        passengers: Some(passengers),
        ..Default::default()
    };
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        g.initialize_path(
            &mut master,
            &mut context,
            &mut owner,
            WaypointPathRequest::default()
        )
        .unwrap_err()
        .id,
        "execution.waypoint.minimum_path_time_nonterminating_cycle_370"
    );
    assert_eq!(owner.speed_reads, 2);
    let geometry = owner.geometry.as_ref().unwrap();
    assert_eq!(geometry.pathfinder_constructor_completions, 2);
    assert_eq!(geometry.pathfinder_calculate_completions, 2);
    let (delayed, paths) = loaded(&[(1, 1, 0)]);
    owner.paths = Some(paths);
    assert_eq!(
        delayed
            .initialize_path(
                &mut master,
                &mut context,
                &mut owner,
                WaypointPathRequest::default()
            )
            .unwrap_err()
            .id,
        "execution.spline.movement_info_owner_176"
    );
    assert_eq!(owner.speed_reads, 3);
    let geometry = owner.geometry.as_ref().unwrap();
    assert_eq!(geometry.pathfinder_constructor_completions, 3);
    assert_eq!(geometry.pathfinder_calculate_completions, 3);
    let args = benilla_sim::source_spline::Args {
        path: vec![[0; 3], [3f32.to_bits(), 4f32.to_bits(), 0]],
        facing: [Some(0); 3],
        flags: 0,
        path_index_offset: 0,
        velocity: 5f32.to_bits(),
        spline_id: 9,
    };
    let spline = &mut context
        .units
        .get_mut(&objects, master.unit)
        .unwrap()
        .movespline;
    spline.initialize(&args).unwrap();
    spline.update_state(500, |_| {}).unwrap();
    let computed = spline.compute_position().unwrap().map(f32::from_bits);
    owner.spline = Some(false);
    assert_eq!(
        delayed
            .send_next(&mut master, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.spline.movement_info_owner_176"
    );
    assert_eq!(
        owner.path_starts.last().unwrap(),
        &[computed[0], computed[1], computed[2]]
    );
    assert_ne!(*owner.path_starts.last().unwrap(), [1.; 3]);
    assert_eq!(owner.speed_reads, 4);
    assert_eq!(
        owner
            .geometry
            .as_ref()
            .unwrap()
            .pathfinder_calculate_completions,
        4
    );
}

#[test]
fn real_current_path_index_pops_one_pending_node_before_arrival() {
    struct BoundPosition<'a>(&'a SourceObjects);
    impl MotionExecutionOwner for BoundPosition<'_> {
        fn waypoint_position(
            &mut self,
            unit: ObjectHandle,
            _: &MotionContext<'_>,
        ) -> Result<[f32; 4]> {
            Ok(self.0.pose(unit)?.0.map(f32::from_bits))
        }
    }
    let (g, _) = loaded(&[(1, 0, 0), (2, 0, 0)]);
    let (objects, mut units, mut master) = setup();
    let fields = units.get_mut(&objects, master.unit).unwrap();
    fields.state = WANDER_MOVING;
    fields
        .movespline
        .initialize(&benilla_sim::source_spline::Args {
            path: vec![[0; 3], [3f32.to_bits(), 4f32.to_bits(), 0]],
            facing: [Some(0); 3],
            flags: 0,
            path_index_offset: 23,
            velocity: 5f32.to_bits(),
            spline_id: 9,
        })
        .unwrap();
    let mut progress = g.progress.get();
    progress.current = 1;
    progress.path_duration = 500;
    g.progress.set(progress);
    g.cursor.set(Some(1));
    g.node_indexes.borrow_mut().extend([23, 24]);
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert!(g
        .update(&mut master, 1, &mut context, &mut BoundPosition(&objects))
        .unwrap());
    assert_eq!(*g.node_indexes.borrow(), [24]);
    assert_eq!(g.progress.get().last_reached, 1);
    assert_eq!(g.progress.get().path_duration, 499);
    assert!(!context
        .units
        .get(&objects, master.unit)
        .unwrap()
        .movespline
        .finalized());
}

#[test]
fn initialize_path_reads_default_entry_before_requesting_the_waypoint_manager() {
    // WaypointMovementGenerator.cpp:39-49. An explicit entry skips GetEntry.
    let (objects, _, mut master) = setup();
    let mut units = SourceUnits::default();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    for (overwrite_entry, expected) in [
        (0, "execution.unit.fields_generation_missing"),
        (901, "execution.waypoint.loaded_waypoint_manager_owner"),
    ] {
        assert_eq!(
            SourceWaypointGenerator::default()
                .initialize_path(
                    &mut master,
                    &mut context,
                    &mut Owner::default(),
                    WaypointPathRequest {
                        overwrite_entry,
                        ..Default::default()
                    },
                )
                .unwrap_err()
                .id,
            expected
        );
    }
}
