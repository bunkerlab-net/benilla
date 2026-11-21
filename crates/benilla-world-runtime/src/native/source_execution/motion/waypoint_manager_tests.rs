use super::*;

fn row(owner: u32, path: u32, point: u32, script: u32) -> SourceWaypointRow {
    SourceWaypointRow {
        ordinal: u64::from(point),
        owner,
        path,
        point,
        node: SourceWaypointNode {
            position: [1., 2., 3.],
            orientation: None,
            delay: 0,
            script,
        },
    }
}
struct Owner;
impl WaypointLoadOwner for Owner {
    fn creature(&mut self, guid: u32) -> Result<Option<(u32, u32)>> {
        Ok(match guid {
            7 => Some((901, 0)),
            8 => Some((902, 2)),
            _ => None,
        })
    }
    fn template(&mut self, entry: u32) -> Result<Option<u32>> {
        Ok((entry == 901).then_some(2))
    }
}
#[test]
fn manager_keeps_source_blacklist_bug_and_diagnostic_order() {
    let mut scripts = WaypointScriptMaps::default();
    scripts.movement.insert(
        10,
        vec![WaypointScriptStep {
            command: 25,
            ..Default::default()
        }],
    );
    scripts.movement.insert(20, vec![]);
    let paths = SourceWaypointPaths::load_manager(
        [
            vec![
                row(7, 0, 1, 10),
                row(8, 0, 0, 0),
                row(8, 0, 1, 0),
                row(9, 0, 0, 0),
            ],
            vec![row(901, 0, 1, 999), row(901, 1, 0, 0), row(999, 0, 1, 0)],
            vec![row(901 << 8, 0, 0, 0), row(901 << 8, 0, 1, 0)],
        ],
        BTreeSet::new(),
        &mut Owner,
        Some(&scripts),
    )
    .unwrap();
    assert_eq!(paths.report.rows, [4, 3, 2]);
    assert_eq!(paths.report.missing_owner, [1, 1, 0]);
    assert_eq!(paths.report.blacklisted_nodes, [2, 2, 0]);
    assert_eq!(paths.report.paths, [1, 0, 1]);
    assert_eq!(paths.report.nodes, [1, 0, 2]);
    assert_eq!(paths.report.statements, WAYPOINT_MANAGER_QUERIES);
    assert_eq!(
        paths
            .report
            .notices
            .iter()
            .map(|n| n.line)
            .collect::<Vec<_>>(),
        [82, 218, 218, 225, 287, 290, 54, 348, 353, 457, 463, 507]
    );
    let (_, path) = paths.select(901, 7, 0, WaypointOrigin::None).unwrap();
    assert_eq!(path.unwrap()[&1].delay, 1000);
    let (origin, path) = paths.select(901, 8, 0, WaypointOrigin::None).unwrap();
    assert_eq!(origin, WaypointOrigin::WaypointPath);
    assert!(path.unwrap().contains_key(&0));
}
#[test]
fn manager_normalizes_only_xy_and_preserves_invalid_z_and_orientation() {
    let mut template = row(901, 0, 1, 0);
    template.node.position = [17067., -17067., f32::INFINITY];
    template.node.orientation = Some(f32::NAN);
    let mut waypoint = row(2, 0, 1, 0);
    waypoint.node.position = [f32::NAN, f32::NEG_INFINITY, 0.];
    let paths = SourceWaypointPaths::load_manager(
        [vec![], vec![template], vec![waypoint]],
        BTreeSet::from([2]),
        &mut Owner,
        None,
    )
    .unwrap();
    let path = paths
        .path_from_origin(901, 0, 0, WaypointOrigin::Entry)
        .unwrap()
        .unwrap();
    assert_eq!(&path[&1].position[..2], &[17066.166, -17066.166]);
    assert!(path[&1].position[2].is_infinite());
    assert!(path[&1].orientation.unwrap().is_nan());
    let path = paths
        .path_from_origin(0, 0, 2, WaypointOrigin::WaypointPath)
        .unwrap()
        .unwrap();
    assert!(path[&1].position[0].is_nan());
    assert_eq!(path[&1].position[1], -17066.166);
    assert_eq!(
        paths
            .report
            .notices
            .iter()
            .map(|n| n.line)
            .collect::<Vec<_>>(),
        [371, 377, 480, 486]
    );
    assert_eq!(paths.report.statements[0], WAYPOINT_MANAGER_QUERIES[0]);
    assert_eq!(
        paths.report.statements[3],
        "UPDATE creature_movement_template SET PositionX = '17066.166016', PositionY = '-17066.166016' WHERE Entry = 901 AND Point = 1 AND PathId = 0"
    );
}
#[test]
fn empty_manager_only_issues_group_counts_and_logs_unused_scripts() {
    let mut scripts = WaypointScriptMaps::default();
    scripts.movement.insert(20, vec![]);
    let paths = SourceWaypointPaths::load_manager(
        Default::default(),
        BTreeSet::new(),
        &mut Owner,
        Some(&scripts),
    )
    .unwrap();
    assert_eq!(
        paths.report.statements,
        [
            WAYPOINT_MANAGER_QUERIES[0],
            WAYPOINT_MANAGER_QUERIES[2],
            WAYPOINT_MANAGER_QUERIES[4]
        ]
    );
    assert_eq!(paths.report.notices[0].line, 507);
    assert_eq!(paths.report.notices[0].script, 20);
}

#[test]
fn repair_sql_matches_promoted_printf_float_and_nonfinite_spelling() {
    for (value, expected) in [
        (-0., "-0.000000"),
        (f32::NAN, "nan"),
        (-f32::NAN, "-nan"),
        (f32::INFINITY, "inf"),
        (f32::NEG_INFINITY, "-inf"),
        (17066.166, "17066.166016"),
    ] {
        assert_eq!(WaypointSqlFloat(value).to_string(), expected);
    }
}

#[test]
fn wrong_motion_diagnostic_refuses_a_disappearing_creature_data_pointer() {
    struct VanishingOwner(bool);
    impl WaypointLoadOwner for VanishingOwner {
        fn creature(&mut self, _: u32) -> Result<Option<(u32, u32)>> {
            Ok(if std::mem::replace(&mut self.0, false) {
                Some((901, 0))
            } else {
                None
            })
        }
        fn template(&mut self, _: u32) -> Result<Option<u32>> {
            panic!("cData->id is dereferenced before the template lookup");
        }
    }
    assert_eq!(
        SourceWaypointPaths::load_manager(
            [vec![row(7, 0, 1, 0)], vec![], vec![]],
            BTreeSet::new(),
            &mut VanishingOwner(true),
            None,
        )
        .err()
        .unwrap()
        .id,
        "execution.waypoint.WaypointManager_279.null_creature_data_undefined"
    );
    let stable = SourceWaypointPaths::load_manager(
        [vec![row(7, 0, 1, 0)], vec![], vec![]],
        BTreeSet::new(),
        &mut Owner,
        None,
    )
    .unwrap();
    assert_eq!(
        stable
            .report
            .notices
            .iter()
            .map(|n| n.line)
            .collect::<Vec<_>>(),
        [287, 290]
    );
}
