// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen resource diagnostics only. No allocation addresses or accepted live execution.
use super::*;
use benilla_native_vmap::live::{FileState, GameObjectModelList};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
#[test]
fn optional_dynamic_stock_resource_census_uses_actual_model_decoding() {
    let (Ok(root), Ok(catalog_root)) = (
        std::env::var("BENILLA_F6_DYNAMIC_F1"),
        std::env::var("BENILLA_F2_CATALOG"),
    ) else {
        println!(
            "SKIP dynamic stock resource census: BENILLA_F6_DYNAMIC_F1/BENILLA_F2_CATALOG absent"
        );
        return;
    };
    let root = Path::new(&root);
    let bytes = std::fs::read(root.join("products-manifest.json")).unwrap();
    assert_eq!(
        Sha256Id::of(&bytes),
        Sha256Id::parse("5d681785ca7cbccd26b453464e5d86a178290ed41caac9bac4c890053f69559f")
            .unwrap()
    );
    let manifest: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    let mut vmap = SourceVMapManager::default();
    for row in manifest {
        let path = row["path"].as_str().unwrap();
        let Some(name) = path.strip_prefix("vmaps/") else {
            continue;
        };
        if !name.ends_with(".vmo") && name != "temp_gameobject_models" {
            continue;
        }
        let bytes = std::fs::read(root.join("products").join(path)).unwrap();
        assert_eq!(bytes.len() as u64, row["bytes"].as_u64().unwrap());
        assert_eq!(
            Sha256Id::of(&bytes),
            Sha256Id::parse(row["sha256"].as_str().unwrap()).unwrap()
        );
        vmap.collision
            .files
            .observe(name, FileState::Present(bytes.into()))
            .unwrap();
    }
    let mut list = GameObjectModelList::default();
    list.load(&vmap.collision.files).unwrap();
    let mut catalog = VerifiedCatalog::open(Path::new(&catalog_root), false).unwrap();
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let mut models = SourceDynamicModels::default();
    models.load_displays(&mut catalog).unwrap();
    models.load_model_list(&vmap).unwrap();
    let templates = catalog
        .execution_db_script_operands("gameobject_template", ["entry", "displayId", "type"])
        .unwrap();
    let mut display_results = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    let mut template_counts = BTreeMap::new();
    let mut transport_counts = BTreeMap::new();
    for [entry, display, kind] in templates {
        let result = *display_results.entry(display).or_insert_with(|| {
            if !models.displays.as_ref().unwrap().contains(&display) {
                return "null_display";
            }
            let Some(bounds) = list.bounds(display) else {
                return "missing_model_list_entry";
            };
            if bounds == [[0.0; 3]; 2] {
                return "zero_bounds";
            }
            match vmap.collision.acquire_model(list.name(display).unwrap()) {
                Ok(Some(handle)) => {
                    vmap.collision.release_model(&handle).unwrap();
                    "loaded"
                }
                Ok(None) => "source_model_read_false",
                Err(benilla_native_vmap::live::Error::ProductUnavailable) => {
                    "execution.vmap.product_unavailable"
                }
                Err(other) => panic!("unexpected model refusal for display {display}: {other:?}"),
            }
        });
        *template_counts.entry(result).or_insert(0u64) += 1;
        if kind == 15 {
            *transport_counts.entry(result).or_insert(0u64) += 1;
        }
        outcomes.insert(entry, result);
    }
    let mut spawn_counts = BTreeMap::new();
    for [_guid, entry] in catalog
        .execution_db_script_operands("gameobject", ["guid", "id"])
        .unwrap()
    {
        *spawn_counts
            .entry(
                outcomes
                    .get(&entry)
                    .copied()
                    .unwrap_or("effective_template_owner"),
            )
            .or_insert(0u64) += 1;
    }
    assert!(vmap.collision.model_inventory().is_empty());
    println!("Frozen raw-template resource census, not effective spawn selection or accepted dynamic execution: {}",
        serde_json::json!({"templates":template_counts,"spawns":spawn_counts,"mo_transport_templates":transport_counts,
            "actual_live_stock_insertions":"unavailable_without_lifecycle_stream",
            "model_list_undefined_reads_reached":0,"cell_undefined_calls_reached":0,"counter_overflow_calls_reached":0,
            "normal_run_cell_undefined_reach":"unavailable_without_lifecycle_stream_and_query_operands"}));
    potential_overlap(&mut catalog, &mut models, &mut vmap, &outcomes, root);
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

fn potential_overlap(
    catalog: &mut VerifiedCatalog,
    models: &mut SourceDynamicModels,
    vmap: &mut SourceVMapManager,
    outcomes: &BTreeMap<u32, &str>,
    root: &Path,
) {
    use crate::native::production_compiler::ObjectMgrSpawnRows;
    use std::collections::BTreeSet;
    let Ok(occurrences) = std::env::var("BENILLA_F2_SPAWN_OCCURRENCES") else {
        println!("SKIP potential dynamic overlap: BENILLA_F2_SPAWN_OCCURRENCES absent");
        return;
    };
    bind_dispatch(vmap, root);
    let mut definitions = BTreeMap::new();
    catalog
        .scan("gameobject_template", 0, 256, |row| {
            let cells = row.cells().collect::<std::result::Result<Vec<_>, _>>()?;
            assert_eq!(cells.len(), 38);
            let text = |i: usize| std::str::from_utf8(cells[i].unwrap()).unwrap();
            definitions.insert(
                text(0).parse::<u32>().unwrap(),
                (
                    text(2).parse::<u32>().unwrap(),
                    text(8).parse::<f32>().unwrap().to_bits(),
                ),
            );
            Ok(())
        })
        .unwrap();
    let mut rows = ObjectMgrSpawnRows::open(catalog, Path::new(&occurrences)).unwrap();
    while rows.next("creature").unwrap().is_some() {}
    let mut maps = BTreeMap::new();
    let mut objects = SourceObjects::default();
    let mut inserted = Vec::new();
    let mut attempts = 0;
    let mut null_models = 0;
    let mut stopped = BTreeMap::new();
    while let Some(row) = rows.next("gameobject").unwrap() {
        if row.map > 1 {
            continue;
        }
        attempts += 1;
        if outcomes.get(&row.entry) != Some(&"loaded") {
            null_models += 1;
            continue;
        }
        let (display, scale_bits) = definitions[&row.entry];
        let handle = objects.construct(1, SourceObjectKind::GameObject).unwrap();
        let identity = MapIdentity {
            map: row.map,
            instance: 0,
            creation_generation: 1,
        };
        let map = maps
            .entry(row.map)
            .or_insert_with(|| SourceDynamicMap::new(identity));
        let object = DynamicGameObject {
            handle,
            display,
            pose: SourcePose(row.pose.map(f32::to_bits)),
            scale_bits,
        };
        match models.operation(
            map,
            vmap,
            object,
            GameObjectModelOperation::ConstructAndInsert,
        ) {
            Ok(true) => inserted.push(models.handle(handle).unwrap().clone()),
            Ok(false) => {
                null_models += 1;
            }
            Err(error) => {
                *stopped.entry(error.id).or_insert(0u64) += 1;
            }
        }
    }
    let mut multi = BTreeSet::new();
    let mut cells = [0u64; 2];
    for map in maps.values_mut() {
        map.update(models, 200).unwrap();
        for node in map.tree.inventory() {
            cells[usize::from(node.pending.len() > 1)] += 1;
            if node.pending.len() > 1 {
                multi.extend(node.pending.iter().map(|h| h.native_generation()));
            }
        }
    }
    let multi_spawns = inserted
        .iter()
        .filter(|h| multi.contains(&h.native_generation()))
        .count();
    println!("potential overlap (all raw spawn rows, ignoring event/pool selection and grid lifetimes): {}",
        serde_json::json!({"continent_rows":attempts, "source_null_or_effective_template_owner":null_models,
            "conditional_raw_spawn_insertions":inserted.len(), "spawns_in_multi_model_cells":multi_spawns,
            "spawns_only_in_single_model_cells":inserted.len()-multi_spawns,
            "single_model_cells":cells[0],"multi_model_cells":cells[1],"refusals":stopped,
            "stock_queries_answered":0,"query_operands":"unavailable","actual_live_occupancy":"unavailable",
            "bound":"potential upper bound for raw GO DB poses, not normal-run reach"}));
    transport_model_insertions(catalog, models, vmap, &mut objects, &definitions);
}

fn transport_model_insertions(
    catalog: &mut VerifiedCatalog,
    models: &mut SourceDynamicModels,
    vmap: &mut SourceVMapManager,
    objects: &mut SourceObjects,
    definitions: &BTreeMap<u32, (u32, u32)>,
) {
    let Ok(occurrences) = std::env::var("BENILLA_F2_DBC_OCCURRENCES") else {
        println!(
            "SKIP raw transport initial-model insertion census: BENILLA_F2_DBC_OCCURRENCES absent"
        );
        return;
    };
    let inputs = catalog
        .execution_transport_inputs(Path::new(&occurrences))
        .unwrap();
    let entries: Vec<_> = inputs
        .raw_gameobjects
        .values()
        .filter(|go| go.kind == 15)
        .map(|go| go.entry)
        .collect();
    struct RawDefinitions(
        Arc<BTreeMap<u32, crate::native::production_compiler::TransportGameObject>>,
    );
    impl TransportDefinitionOwner for RawDefinitions {
        fn gameobject_definitions(
            &mut self,
        ) -> Result<Arc<BTreeMap<u32, crate::native::production_compiler::TransportGameObject>>>
        {
            Ok(Arc::clone(&self.0))
        }
    }
    let mut paths = SourceTransportTemplates::default();
    paths
        .load_templates_from_catalog(
            catalog,
            Path::new(&occurrences),
            &mut RawDefinitions(Arc::new(inputs.raw_gameobjects)),
            &mut |_| Ok(()),
        )
        .unwrap();
    let mut maps = BTreeMap::new();
    let mut inserted = 0;
    let mut refusals = BTreeMap::new();
    for entry in entries {
        let path = paths.template(entry).unwrap().unwrap();
        let frame = &path.key_frames[0];
        let node = &path.nodes[frame.node];
        let identity = MapIdentity {
            map: node.map,
            instance: 0,
            creation_generation: 1,
        };
        let map = maps
            .entry(node.map)
            .or_insert_with(|| SourceDynamicMap::new(identity));
        let handle = objects.construct(1, SourceObjectKind::Transport).unwrap();
        let (display, scale_bits) = definitions[&entry];
        let object = DynamicGameObject {
            handle,
            display,
            scale_bits,
            pose: SourcePose([
                node.position[0],
                node.position[1],
                node.position[2],
                frame.orientation,
            ]),
        };
        match models.operation(
            map,
            vmap,
            object,
            GameObjectModelOperation::ConstructAndInsert,
        ) {
            Ok(true) => {
                inserted += 1;
            }
            Ok(false) => {
                *refusals
                    .entry("source_null_model".to_owned())
                    .or_insert(0u64) += 1;
            }
            Err(error) => {
                *refusals.entry(error.id).or_insert(0u64) += 1;
            }
        }
    }
    println!("Raw transport initial-model calls, not full MO registration or accepted lifecycle execution: {}",
        serde_json::json!({"conditional_model_insertions":inserted,"refusals":refusals,"source_allocation_addresses_supplied":0}));
}
