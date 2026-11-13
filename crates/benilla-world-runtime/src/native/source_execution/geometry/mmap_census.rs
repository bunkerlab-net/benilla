// SPDX-License-Identifier: GPL-3.0-or-later
//! Diagnostic frozen inventory, not a recorded tile insertion order or product admission.
use super::*;
#[test]
fn frozen_products_load_or_name_each_refusal_and_real_no_nav_branch() {
    let Some(root) = std::env::var_os("BENILLA_F6_MMAP_F1") else {
        println!("SKIP frozen MMap census: BENILLA_F6_MMAP_F1 absent");
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
    let config_bytes =
        std::fs::read(root.join("sealing/source-choice/seal16-full-01/mangosd.conf")).unwrap();
    let approved =
        Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap();
    let mut manager = SourceMMapManager::default();
    manager
        .initialize_recorded_world(&config_bytes, &approved)
        .unwrap();
    assert_eq!(manager.configuration.as_ref().unwrap().1, [269, 270, 271]);
    let mut meshes = 0;
    let mut candidates = Vec::new();
    let mut short_params = 0;
    let mut short_headers = 0;
    let mut mmap_names = BTreeSet::new();
    for row in &manifest {
        let path = row["path"].as_str().unwrap();
        let Some(name) = path.strip_prefix("mmaps/") else {
            continue;
        };
        let hash = Sha256Id::parse(row["sha256"].as_str().unwrap()).unwrap();
        let path = root.join("products").join(path);
        if name.ends_with(".mmap") {
            let map = name[..3].parse().unwrap();
            short_params += usize::from(std::fs::metadata(&path).unwrap().len() < 28);
            manager.files.retain_map(map, &path, hash).unwrap();
            assert_eq!(manager.load_map_instance(map, 0).unwrap(), MMapLoad::Loaded);
            meshes += 1;
        } else if !name.starts_with("go") {
            let map = name[..3].parse::<u32>().unwrap();
            let x = name[3..5].parse::<u16>().unwrap();
            let y = name[5..7].parse::<u16>().unwrap();
            mmap_names.insert((map, x, y));
            let length = std::fs::metadata(&path).unwrap().len();
            short_headers += usize::from((1..20).contains(&length));
            manager.files.retain_tile(map, x, y, &path, hash).unwrap();
            candidates.push((map, x, y));
        }
    }
    // Explicit diagnostic order only. Source loadAllMapTiles iteration is not inferred.
    candidates.sort_unstable();
    let mut refused = BTreeMap::<String, usize>::new();
    let mut loaded = 0;
    for (map, x, y) in candidates.iter().copied() {
        match manager.load_tile(map, x, y) {
            Ok(MMapLoad::Loaded) => loaded += 1,
            Ok(outcome) => panic!(
                "unexpected source load false for authenticated product {map}/{x}/{y}: {outcome:?}"
            ),
            Err(error) => {
                *refused.entry(error.id).or_default() += 1;
            }
        }
    }
    assert_eq!(meshes, 2);
    assert_eq!(candidates.len(), 1294);
    assert_eq!(loaded + refused.values().sum::<usize>(), 1294);
    assert_eq!((short_params, short_headers), (0, 0));
    assert_eq!(manager.tile_count(), loaded);
    let mut absent = 0;
    let mut first_absent = None;
    for row in &manifest {
        let path = row["path"].as_str().unwrap();
        let Some(name) = path.strip_prefix("maps/") else {
            continue;
        };
        if name.len() != 11 {
            continue;
        }
        let map = name[..3].parse::<u32>().unwrap();
        if manager.mesh(map).is_none() {
            continue;
        }
        let x = name[3..5].parse::<u16>().unwrap();
        let y = name[5..7].parse::<u16>().unwrap();
        if mmap_names.contains(&(map, x, y)) {
            continue;
        }
        let missing = root
            .join("products/mmaps")
            .join(format!("{map:03}{x:02}{y:02}.mmtile"));
        assert_eq!(
            std::fs::metadata(missing).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        manager.files.source_tile_absent(map, x, y).unwrap();
        assert_eq!(manager.load_tile(map, x, y).unwrap(), MMapLoad::TileAbsent);
        assert!(!manager.is_tile_loaded(map, x, y));
        absent += 1;
        first_absent.get_or_insert((map, x, y));
    }
    assert!(absent > 0);
    let mut availability = BTreeMap::<String, BTreeMap<String, usize>>::new();
    if let Some(catalog_root) = std::env::var_os("BENILLA_F5_AI_CATALOG") {
        let mut catalog = super::super::super::production_compiler::VerifiedCatalog::open(
            Path::new(&catalog_root),
            false,
        )
        .unwrap();
        let maps = catalog
            .execution_event_ai_operands("creature", ["guid", "map"])
            .unwrap()
            .into_iter()
            .map(|(_, [guid, map])| (guid, map))
            .collect::<BTreeMap<_, _>>();
        for spawn in catalog.execution_motion_spawn_operands().unwrap() {
            let movement = if spawn.radius >= 0.0 && spawn.radius == 0.0 && spawn.movement_type == 1
            {
                0
            } else {
                spawn.movement_type
            };
            let family = match movement {
                1 => "wander",
                2 => "waypoint",
                _ => continue,
            };
            let map = maps[&spawn.guid];
            let rows = availability.entry(family.into()).or_default();
            *rows
                .entry("raw_constructor_opportunities".into())
                .or_default() += 1;
            let outcome = if manager
                .pathfinding_enabled(map, MMapForcing::Default)
                .unwrap()
            {
                if let Some(query) = manager.query(map, 0) {
                    assert_eq!(manager.query_capacity(map, &query).unwrap(), 1024);
                    "query_available"
                } else {
                    "source_null_query"
                }
            } else {
                "disabled_default_policy"
            };
            *rows.entry(outcome.into()).or_default() += 1;
        }
        assert_eq!(
            availability["wander"]["raw_constructor_opportunities"],
            43_119
        );
        assert_eq!(
            availability["waypoint"]["raw_constructor_opportunities"],
            2_931
        );
    }
    println!(
        "F6_MMAP_CENSUS {}",
        serde_json::json!({
            "scope":"frozen diagnostic products, lexical tile order, no new lineage or F1 acceptance",
            "meshes":meshes,"tile_products":candidates.len(),"loaded":loaded,"refusals":refused,
            "real_no_nav_tiles":absent,"first_no_nav":first_absent,
            "undefined_short_params":short_params,"undefined_partial_headers":short_headers,
            "undefined_config_atoi":0,
            "query_capacity":manager.query_capacity(0, &manager.query(0,0).unwrap()).unwrap(),
            "raw_query_availability_not_constructor_execution":availability,
            "pathfinder_constructor_completions_before_after":[0,0],
            "next_owner":"F6.08a PathFinder::createFilter and actual motion constructor binding",
            "home_recorded_constructor_opportunities":"unavailable; no call stream is inferred from static spawns",
        })
    );
}
