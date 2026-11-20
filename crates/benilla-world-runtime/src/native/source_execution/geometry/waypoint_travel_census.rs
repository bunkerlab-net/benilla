// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw selected-path diagnostics only, never recorded movement or F1 acceptance.
use super::*;
use std::collections::VecDeque;
type Probe = (
    u32,
    u32,
    [f32; 4],
    SourceObjects,
    SourceUnits,
    SourcePathFinder,
);

pub(super) fn run(
    geometry: &mut SourceTerrainManager,
    catalog: &mut crate::native::production_compiler::VerifiedCatalog,
    templates: &BTreeMap<u32, CreatureAiTemplateOperands>,
    paths: &[BTreeMap<u32, SourceWaypointPath>],
    probes: Vec<Probe>,
) -> serde_json::Value {
    let raw_templates = catalog
        .execution_loaded_creature_rows()
        .unwrap()
        .into_iter()
        .map(|row| (row.words[0], row))
        .collect::<BTreeMap<_, _>>();
    let spawn_templates = LoadedCreatureSpawnTemplates::load(
        catalog.execution_creature_spawn_templates().unwrap(),
        &Default::default(),
    );
    let mut spawn_ids = BTreeMap::new();
    if let Some(root) = std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES") {
        let mut rows =
            crate::native::production_compiler::ObjectMgrSpawnRows::open(catalog, Path::new(&root))
                .unwrap();
        while let Some(row) = rows.next("creature").unwrap() {
            spawn_ids.insert(row.guid, row.spawn_template);
        }
    }
    let mut first = BTreeMap::<String, usize>::new();
    let mut next = BTreeMap::<String, usize>::new();
    let opportunities = probes.len();
    for (guid, entry, pose, objects, mut units, pathfinder) in probes {
        let mut first_complete = false;
        let result = (|| {
            let raw = &raw_templates[&entry];
            let mut movement = SourceCreatureMovement::construct();
            let spawn = spawn_ids
                .get(&guid)
                .ok_or_else(|| refuse("diagnostic.waypoint.spawn_occurrence_absent"))
                .and_then(|id| {
                    spawn_templates.rows().get(id).map(Some).ok_or_else(|| {
                        refuse("execution.object_mgr.loaded_creature_spawn_data_template_owner")
                    })
                });
            // Fixed SQLStorages layout, SpeedWalk/SpeedRun. Zero needs the selected model.
            for (kind, column) in [(0, 29), (1, 30)] {
                let rate = f32::from_bits(raw.words[column]);
                if rate == 0. {
                    return Err(refuse("execution.movement.selected_model_rates_owner"));
                }
                movement.set_speed_rate(kind, rate);
            }
            let mut state = 0;
            movement.init_entry_flags(
                &templates[&entry],
                false,
                false,
                spawn,
                || {
                    Err(refuse(
                        "execution.movement.creation_swimmable_collision_height_owner_441",
                    ))
                },
                |movement| movement.set_walk(&mut state, 0, false, true).map(|_| ()),
            )?;
            let speed = movement.speed()?;
            let selected = paths[0]
                .get(&guid)
                .or_else(|| paths[1].get(&entry.wrapping_shl(8)))
                .or_else(|| paths[2].get(&entry.wrapping_shl(8)))
                .unwrap();
            let (&cursor, node) = selected.first_key_value().unwrap();
            let mut generated = vec![[pose[0], pose[1], pose[2]]];
            let travel = source_waypoint_append_path(
                &mut generated,
                &pathfinder.points,
                node.position,
                speed,
            )?;
            *first.entry("complete".into()).or_default() += 1;
            first_complete = true;
            let context = MotionContext {
                objects: &objects,
                units: &mut units,
            };
            source_waypoint_lookahead(
                selected,
                cursor,
                &mut generated,
                travel,
                &mut VecDeque::new(),
                |cursor, _| {
                    Ok(selected
                        .range((
                            std::ops::Bound::Excluded(cursor),
                            std::ops::Bound::Unbounded,
                        ))
                        .next()
                        .or_else(|| selected.first_key_value())
                        .map(|(&key, _)| key))
                },
                |generated, end| {
                    let start = *generated.last().unwrap();
                    let mut pf =
                        SourcePathFinder::construct(geometry, pathfinder.unit, &context, true)?;
                    pf.calculate(geometry, &context, Ok(None), start, end, true, false)?;
                    source_waypoint_append_path(generated, &pf.points, end, speed)
                },
            )?;
            SourceMoveSplineInit::construct(pathfinder.unit, &context).map(|_| ())
        })();
        let id = result.unwrap_err().id;
        if !first_complete {
            *first.entry(id.clone()).or_default() += 1;
        }
        *next.entry(id).or_default() += 1;
    }
    assert_eq!(first.values().sum::<usize>(), opportunities);
    assert_eq!(next.values().sum::<usize>(), opportunities);
    serde_json::json!({"scope":"successful first raw PathFinder calculations, with source constructor defaults and raw template/spawn creation writes; swimming refuses without selected-model collision height; no ScriptMgr, live control, recorded invocation or F1 claim", "opportunities":opportunities, "first_build_int_path":first,"next_refusals":next,
        "creation_dependencies":first.iter().filter(|(id,_)| id.starts_with("execution.movement.") || id.starts_with("execution.object_mgr.") || id.starts_with("diagnostic.")).collect::<BTreeMap<_,_>>(),
        "build_int_path_complete":first.get("complete").copied().unwrap_or(0),
        "lookahead_cycle":next.get("execution.waypoint.minimum_path_time_nonterminating_cycle_370").copied().unwrap_or(0),
        "spline_constructor_flags_refusal":next.get("execution.spline.movement_info_owner_176").copied().unwrap_or(0),
        "ub_279": next.get("execution.waypoint.travel_conversion_undefined_279").copied().unwrap_or(0),
        "ub_284": next.get("execution.waypoint.travel_conversion_undefined_284").copied().unwrap_or(0),
        "zero_speed_279": next.get("execution.waypoint.zero_speed_conversion_undefined_279").copied().unwrap_or(0),
        "zero_speed_284": next.get("execution.waypoint.zero_speed_conversion_undefined_284").copied().unwrap_or(0),
        "empty_path_back_259":next.get("execution.waypoint.path_back_undefined_259").copied().unwrap_or(0)})
}
