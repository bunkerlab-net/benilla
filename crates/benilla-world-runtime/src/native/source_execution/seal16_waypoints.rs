// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:1248 owns one WaypointManager shared by both continent jobs.
use super::*;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use std::rc::Rc;

pub(super) fn load<E: RawGenerator, S: FnMut(TraceEvent)>(
    catalog: &mut VerifiedCatalog,
    spawns: &SourceEffectiveSpawns,
    templates: &LoadedCreatureTemplates,
    scripts: &SourceDbScriptMaps,
    geometry: &mut SourceTerrainManager,
    random: &mut SourceRandom<'_, E, S>,
) -> Result<Rc<SourceWaypointPaths>> {
    let mut paths = SourceWaypointPaths::load_startup_catalog(
        catalog, spawns, templates, scripts, geometry, random,
    )?;
    // ScriptDevAI/system/system.cpp:75-114 registers external paths later.
    paths.load_scriptdev(
        catalog.execution_waypoint_rows("script_waypoint")?,
        &templates.rows().iter().map(|row| row.words[0]).collect(),
    );
    let mut notices = BTreeMap::<u32, usize>::new();
    for notice in &paths.report.notices {
        *notices.entry(notice.line).or_default() += 1;
    }
    println!(
        "F6_WAYPOINT_MANAGER rows={:?} missing_owner={:?} blacklisted_nodes={:?} paths={:?} nodes={:?} notices={notices:?}",
        paths.report.rows,
        paths.report.missing_owner,
        paths.report.blacklisted_nodes,
        paths.report.paths,
        paths.report.nodes
    );
    Ok(Rc::new(paths))
}

pub(super) fn census(
    paths: &SourceWaypointPaths,
    spawns: &SourceEffectiveSpawns,
    groups: &SourceSpawnGroups,
    catalog: &mut VerifiedCatalog,
) {
    let alternatives = super::super::motion_census_tests::MotionEntryAlternatives::load(catalog);
    let mut formation_paths = BTreeMap::new();
    for group in groups.definitions().unwrap().values() {
        if group.kind() != GridSpawnKind::Creature {
            continue;
        }
        for member in &group.members {
            if let Some(path) = group.waypoint_formation_path(member.guid) {
                formation_paths.insert(member.guid, path);
            }
        }
    }
    // Per creature: GUID, entry, waypoint_path, missing, mixed, unresolved.
    // Alternatives are availability checks, never a fabricated random choice.
    let mut defaults = [[0usize; 6]; 2];
    let mut effective = [[0usize; 6]; 2];
    let mut null_origin_ub = 0;
    let mut fixed_null_origin_ub = 0;
    let mut variable = Vec::new();
    for (key, data) in spawns.pre_terrain_map() {
        if key.kind != GridSpawnKind::Creature || !matches!(data.row.movement, 2..=4) {
            continue;
        }
        let entries = alternatives.resolve(key.guid, data.row.entry);
        let mut choices = BTreeSet::new();
        for entry in entries {
            let (origin, path) = paths
                .select(entry, key.guid, 0, WaypointOrigin::None)
                .unwrap();
            choices.insert(if path.is_none() {
                3
            } else {
                match origin {
                    WaypointOrigin::Guid => 0,
                    WaypointOrigin::Entry => 1,
                    WaypointOrigin::WaypointPath => 2,
                    _ => unreachable!(),
                }
            });
            let undefined = usize::from(paths.default_path(entry, key.guid, None).is_err());
            if data.row.movement == 3 {
                fixed_null_origin_ub += undefined;
            } else {
                null_origin_ub += undefined;
            }
        }
        if data.row.movement == 3 {
            continue;
        }
        let motion = usize::from(data.row.movement == 4);
        let branch = match choices.len() {
            0 => 5,
            1 => *choices.first().unwrap(),
            _ => 4,
        };
        defaults[motion][branch] += 1;
        let effective_branch = if let Some(path) = formation_paths.get(&key.guid) {
            if paths
                .path_from_origin(0, key.guid, *path as i32, WaypointOrigin::WaypointPath)
                .unwrap()
                .is_some()
            {
                2
            } else {
                3
            }
        } else {
            branch
        };
        effective[motion][effective_branch] += 1;
        if branch >= 4 {
            variable.push((key.guid, choices));
        }
    }
    println!("F6_WAYPOINT_PATH_CENSUS defaults={defaults:?} formation_aware={effective:?} variable={variable:?} null_origin_waypoint_fallback_potential_reach={null_origin_ub} fixed_null_origin_undefined_stock_reach={fixed_null_origin_ub} startup_null_origin_reach=0");
    assert_eq!(defaults[0].iter().sum::<usize>(), 2931);
    assert_eq!(defaults[1].iter().sum::<usize>(), 335);
    println!("F6_CAST_QUERY_STOCK_DIAGNOSTIC raw_spawn_waypoint_defaults={} live_spell_slots=not_inferred executed_map1_query_reported_separately", defaults[0].iter().sum::<usize>());
    // WaypointMovementGenerator.cpp:45 always passes &m_PathOrigin.
    assert_eq!(null_origin_ub, 0);
    assert_eq!(fixed_null_origin_ub, 0);
}
