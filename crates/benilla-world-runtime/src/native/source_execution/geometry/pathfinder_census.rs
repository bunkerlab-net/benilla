// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw-pose diagnostic opportunities, not a recorded invocation stream or F1 acceptance.
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
use std::collections::BTreeMap;
pub(in super::super) fn run(
    root: &Path,
    manifest: &[serde_json::Value],
    mmap: SourceMMapManager,
) -> serde_json::Value {
    let Some(catalog_root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        return serde_json::json!({"skipped":"catalog absent"});
    };
    let mut catalog = super::super::super::production_compiler::VerifiedCatalog::open(
        Path::new(&catalog_root),
        false,
    )
    .unwrap();
    let mut geometry = SourceTerrainManager::default();
    geometry.mmap = mmap;
    geometry.dbc = SourceTerrainDbc::catalog(&mut catalog).unwrap();
    geometry.liquid_dbc = SourceLiquidDbc::catalog(&mut catalog).unwrap();
    let config =
        std::fs::read(root.join("sealing/source-choice/seal16-full-01/mangosd.conf")).unwrap();
    geometry
        .vmap
        .initialize_recorded_world(
            geometry.dbc.linked_zones.keys().copied(),
            &config,
            &Sha256Id::of(&config),
        )
        .unwrap();
    bind_dispatch(&mut geometry.vmap, root);
    for row in manifest {
        let name = row["path"].as_str().unwrap();
        let path = root.join("products").join(name);
        let expected = Sha256Id::parse(row["sha256"].as_str().unwrap()).unwrap();
        if let Some(tile) = name
            .strip_prefix("maps/")
            .and_then(|s| s.strip_suffix(".map"))
        {
            geometry
                .files
                .retain_present(
                    tile[..3].parse().unwrap(),
                    tile[3..5].parse().unwrap(),
                    tile[5..7].parse().unwrap(),
                    &path,
                    &expected,
                )
                .unwrap();
        } else if let Some(name) = name.strip_prefix("vmaps/") {
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(Sha256Id::of(&bytes), expected);
            geometry
                .vmap
                .collision
                .files
                .observe(
                    name,
                    benilla_native_vmap::live::FileState::Present(bytes.into()),
                )
                .unwrap();
        }
    }
    let maps = catalog
        .execution_event_ai_operands("creature", ["guid", "map"])
        .unwrap()
        .into_iter()
        .map(|(_, [guid, map])| (guid, map))
        .collect::<BTreeMap<_, _>>();
    let templates = catalog_creature_ai_census_inputs(&mut catalog)
        .unwrap()
        .into_iter()
        .map(|t| (t.entry, t))
        .collect::<BTreeMap<_, _>>();
    let mut census = BTreeMap::<String, BTreeMap<String, usize>>::new();
    let mut unwritten = BTreeMap::<String, usize>::new();
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    for spawn in catalog.execution_motion_spawn_operands().unwrap() {
        let family = match spawn.movement_type {
            1 if spawn.radius != 0. => "wander",
            2 => "waypoint",
            _ => continue,
        };
        let row = census.entry(family.into()).or_default();
        *row.entry("opportunities".into()).or_default() += 1;
        let Some(template) = templates.get(&spawn.entry) else {
            *row.entry("execution.pathfinder.effective_template_owner".into())
                .or_default() += 1;
            continue;
        };
        let map = maps[&spawn.guid];
        // TerrainInfo birth is explicit diagnostic setup, never a constructor draw.
        geometry.load_terrain(map, &mut random).unwrap();
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let object = objects.get_mut(unit).unwrap();
        object.map = Some(MapIdentity {
            map,
            instance: 0,
            creation_generation: 1,
        });
        object.pose = Some(SourcePose(spawn.pose.map(f32::to_bits)));
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, SourceUnitFields::creature(template))
            .unwrap();
        let before = random.cursor();
        match SourcePathFinder::construct(
            &mut geometry,
            unit,
            &MotionContext {
                objects: &objects,
                units: &mut units,
            },
            family == "waypoint",
        ) {
            Ok(path) => {
                *row.entry("constructed".into()).or_default() += 1;
                let reason = if family == "wander" {
                    "execution.motion.client_controlling_owner"
                } else {
                    "execution.waypoint.f6_pathfinder_calculate_owner"
                };
                *row.entry(format!("after_constructor:{reason}"))
                    .or_default() += 1;
                if matches!(path.default_query, SourceDefaultQuery::Unwritten) {
                    *unwritten.entry(family.into()).or_default() += 1;
                }
            }
            Err(error) => {
                *row.entry(error.id).or_default() += 1;
            }
        }
        assert_eq!(random.cursor(), before, "constructor RNG span");
    }
    assert_eq!(census["wander"]["opportunities"], 43119);
    assert_eq!(census["waypoint"]["opportunities"], 2931);
    for row in census.values() {
        let complete = row.get("constructed").copied().unwrap_or(0);
        let failed = row
            .iter()
            .filter(|(reason, _)| reason.starts_with("execution."))
            .map(|(_, count)| count)
            .sum::<usize>();
        assert_eq!(complete + failed, row["opportunities"]);
    }
    serde_json::json!({"scope":"raw template/pose constructor probes with frozen products and diagnostic TerrainInfo birth/order; not effective spawn selection, recorded motion calls, or F1 equality",
        "before":{"wander_constructed":0,"waypoint_constructed":0},"after":census,
        "distance_intent":SourcePathFinder::DISTANCE_INTENT,
        "completed_with_unwritten_default_query":unwritten,"undefined_default_query_reads_reached":0,
        "home":"recorded opportunities unavailable; authored Home constructor only",
        "constructor_completions":geometry.pathfinder_constructor_completions})
}

fn bind_dispatch(vmap: &mut SourceVMapManager, root: &Path) {
    let recording = root.join("sealing/source-choice/seal16-full-01");
    let bytes = std::fs::read(recording.join("actual-process-environment.json")).unwrap();
    assert_eq!(
        Sha256Id::of(&bytes),
        Sha256Id::parse("4b7d35a74a48e0829d942539b97f940f05b4d57416eae71a3caf4dd6269859b5")
            .unwrap()
    );
    let receipt: benilla_sim::source_sine::ProcessReceipt = serde_json::from_slice(&bytes).unwrap();
    for (name, expected) in [
        (
            "acquisition-input-manifest.json",
            &receipt.source_acquisition_bindings.manifest,
        ),
        (
            "trace-schema.json",
            &receipt.source_acquisition_bindings.schema,
        ),
        (
            "instrumentation-identity.json",
            &receipt.source_acquisition_bindings.instrumentation,
        ),
        (
            "run/trace.tsv.process-environment.bin",
            &receipt.source_process_receipt_sha256,
        ),
    ] {
        assert_eq!(
            Sha256Id::of(&std::fs::read(recording.join(name)).unwrap()),
            Sha256Id::parse(expected).unwrap()
        );
    }
    vmap.bind_rotation_dispatch(16, &receipt.source_acquisition_bindings, &receipt)
        .unwrap();
}
