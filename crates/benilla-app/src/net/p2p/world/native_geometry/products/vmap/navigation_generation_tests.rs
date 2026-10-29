// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine authored MPQ capture through generation and the production Detour reader.
use super::super::super::nav_generation::{
    assert_navigation_bytes, GenerationError, PreparedNavigationInput,
};
use super::*;
use std::time::{Duration, Instant};
#[path = "fresh_single_grid_tests.rs"]
mod fresh_single_grid_tests;
#[path = "positive_tile_load_tests.rs"]
mod positive_tile_load_tests;
fn oracle(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/native-navigation/fixtures")
            .join(name),
    )
    .unwrap()
}
pub(super) fn ridge_entries() -> Vec<Entry> {
    placed_entries(oracle("authored/ridge.adt.test-fixture"))
}
fn placed_entries(mut adt: Vec<u8>) -> Vec<Entry> {
    let mut entries = source_height_tests::shared_entries();
    for (field, tag, payload) in [
        (20, b"MWMO", format!("{ROOT}\0").into_bytes()),
        (32, b"MODF", fixture("hull.modf.test-fixture")),
    ] {
        let offset = u32::try_from(adt.len() - 20).unwrap();
        adt[20 + field..24 + field].copy_from_slice(&offset.to_le_bytes());
        adt.extend(chunk(tag, &payload));
    }
    *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() = raw_entry(ADT, adt);
    entries
}
fn cardinal_entries(neighbor_adt: Vec<u8>) -> Vec<Entry> {
    let mut entries = ridge_entries();
    let mut wdt = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures/source.wdt.test-fixture"),
    )
    .unwrap();
    wdt[60..60 + 64 * 64 * 8].fill(0);
    for (x, y) in [(30usize, 31usize), (29, 31)] {
        let offset = 60 + (y * 64 + x) * 8;
        wdt[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
    }
    *entries
        .iter_mut()
        .find(|e| e.name.ends_with("Selected.wdt"))
        .unwrap() = raw_entry("World\\Maps\\Selected\\Selected.wdt", wdt);
    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_29_31.adt",
        neighbor_adt,
    ));
    entries
}

pub(super) fn helper() -> PathBuf {
    let mut executable = std::env::current_exe().unwrap();
    executable.pop();
    executable.pop();
    executable.push(format!(
        "benilla-native-navigation{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(executable.is_file(),"Build benilla-native-nav-generator before scoped geometry tests; workspace tests and CI build it as an integration-test binary");
    executable
}
fn assert_original_routes(mesh: &mut benilla_native_nav::Mesh, expected: &serde_json::Value) {
    for path in expected["output"]["paths"].as_array().unwrap() {
        let point = |name: &str| {
            let values = path[name].as_array().unwrap();
            let bits: Vec<u32> = values
                .iter()
                .map(|v| u32::from_str_radix(v.as_str().unwrap(), 16).unwrap())
                .collect();
            [bits[2], bits[0], bits[1]]
        };
        let result = mesh
            .route(
                point("start_bits"),
                point("end_bits"),
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            result.polygons,
            path["corridor"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect::<Vec<_>>()
        );
        let straight: Vec<u32> = path["straight_bits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u32::from_str_radix(v.as_str().unwrap(), 16).unwrap())
            .collect();
        let expected_corners: Vec<[u32; 3]> = straight
            .as_chunks::<3>()
            .0
            .iter()
            .map(|v| [v[2], v[0], v[1]])
            .collect();
        assert_eq!(
            result.corners.iter().map(|c| c.point).collect::<Vec<_>>(),
            expected_corners
        );
    }
}

#[test]
fn captured_ridge_and_wmo_generate_original_bytes_and_load_production_queries() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let install = Install::new(&ridge_entries());
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let discovery = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = discovery
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let (params, tile) = output.products();
    assert_navigation_bytes(params, &oracle("ridge_wmo/000.mmap"), "ridge_wmo/000.mmap");
    assert_navigation_bytes(
        tile,
        &oracle("ridge_wmo/0003130.mmtile"),
        "ridge_wmo/0003130.mmtile",
    );
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let mut mesh = output.load().unwrap();
    assert_original_routes(&mut mesh, &expected);
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
    assert!(matches!(
        input.generate(&helper(), Instant::now(), || false),
        Err(GenerationError::Deadline)
    ));
    assert!(matches!(
        PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || true),
        Err(GenerationError::Cancelled)
    ));
    let mut changed = ridge_entries();
    changed.push(raw_entry(NEW_ADT, vec![1, 2, 3]));
    install.write(&changed);
    assert!(input.verify_unchanged(|| false).is_err());
    // Already generated bytes remain an immutable historical result.
    output.load().unwrap();
}

#[test]
fn extracted_neighbor_inventory_changes_origin_and_preserves_historical_output() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let mut entries = ridge_entries();
    let neighbors = [
        (31, 31, "ridge_xplus"),
        (29, 31, "ridge_xminus"),
        (30, 32, "ridge_yplus"),
        (30, 30, "ridge_yminus"),
    ];
    let mut wdt = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures/source.wdt.test-fixture"),
    )
    .unwrap();
    wdt[60..60 + 64 * 64 * 8].fill(0);
    for (x, y) in [(30usize, 31usize), (31, 31), (29, 31), (30, 32), (30, 30)] {
        let offset = 60 + (y * 64 + x) * 8;
        wdt[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
    }
    *entries
        .iter_mut()
        .find(|e| e.name.ends_with("Selected.wdt"))
        .unwrap() = raw_entry("World\\Maps\\Selected\\Selected.wdt", wdt);
    for (x, y, fixture_name) in neighbors {
        let name = format!("World\\Maps\\Selected\\Selected_{x}_{y}.adt");
        entries.push(raw_entry(
            &name,
            oracle(&format!("authored/{fixture_name}.adt.test-fixture")),
        ));
    }
    let install = Install::new(&entries);
    let discovery = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = discovery
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let selected = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let old = PreparedNavigationInput::from_source(&selected, &bundle, TILE, || false)
        .unwrap()
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let all = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[(31, 30), (31, 31), (31, 29), (32, 30), (30, 30)]
            .map(|(x, y)| benilla_native_terrain::Tile { x, y }),
    )
    .unwrap();
    let expanded = PreparedNavigationInput::from_source(&all, &bundle, TILE, || false)
        .unwrap()
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    assert_ne!(old.revision(), expanded.revision());
    assert_ne!(old.products().0, expanded.products().0);
    assert_navigation_bytes(
        expanded.products().0,
        &oracle("ridge_neighbors_wmo/000.mmap"),
        "ridge_neighbors_wmo/000.mmap",
    );
    assert_navigation_bytes(
        expanded.products().1,
        &oracle("ridge_neighbors_wmo/0003130.mmtile"),
        "ridge_neighbors_wmo/0003130.mmtile",
    );
    assert_navigation_bytes(
        old.products().0,
        &oracle("ridge_wmo/000.mmap"),
        "ridge_wmo/000.mmap",
    );
    old.load().unwrap();
    expanded.load().unwrap();
}

#[test]
fn captured_liquid_generation_matches_original_but_preserves_dry_reader_refusal() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let terrain_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../benilla-native-terrain/tools/fixtures");
    let entries =
        placed_entries(fs::read(terrain_root.join("liquid_full_types.adt.test-fixture")).unwrap());
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    assert_eq!(
        terrain.tiles().next().unwrap().1,
        fs::read(terrain_root.join("liquid_full_types.map")).unwrap()
    );
    let discovery = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = discovery
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let original_map = oracle("liquid_full_types_wmo/000.mmap");
    let original_tile = oracle("liquid_full_types_wmo/0003130.mmtile");
    let manifest: serde_json::Value = serde_json::from_slice(&oracle("manifest.json")).unwrap();
    input.assert_original_products_for_test(
        &helper(),
        manifest["cases"]["liquid_full_types_wmo"]["helper_request"]["sha256"]
            .as_str()
            .unwrap(),
        &original_map,
        &original_tile,
    );
    // Original rasterization rounds the flat liquid surface to cell 46:
    // minY + 46 * ch = 13.26666355, above the source input maximum 13.25.
    // The current base-vertex envelope refuses it before inspecting wet flags.
    let original_max_y = f32::from_le_bytes(original_tile[108..112].try_into().unwrap());
    let original_first_y = f32::from_le_bytes(original_tile[124..128].try_into().unwrap());
    assert_eq!(original_max_y, 13.25);
    assert_eq!(
        original_first_y.to_bits(),
        (1.0f32 + 46.0 * (0.2666666f32)).to_bits()
    );
    assert!(original_first_y > original_max_y);
    assert!(matches!(
        benilla_native_nav::Mesh::load(
            &original_map,
            &[benilla_native_nav::TileInput {
                grid: [31, 30],
                bytes: &original_tile
            }],
        ),
        Err(benilla_native_nav::Error::Format)
    ));
    let mut observed = None;
    let generated = input.generate_observed(
        &helper(),
        Instant::now() + Duration::from_secs(60),
        || false,
        |summary| observed = Some(summary),
    );
    let summary = observed.expect("source generation metadata precedes reader refusal");
    assert_eq!(
        summary.status,
        benilla_native_nav_generator::GenerationStatus::Generated
    );
    assert_eq!(summary.polygon_count, 998);
    assert_eq!(
        summary.output_bytes,
        original_map.len() + original_tile.len()
    );
    assert!(matches!(
        generated,
        Err(GenerationError::Native(benilla_native_nav::Error::Format))
    ));
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn one_cardinal_neighbor_changes_generation_without_changing_map_origin() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let entries = cardinal_entries(oracle("authored/ridge_xminus.adt.test-fixture"));
    let install = Install::new(&entries);
    let before = install.bytes();
    let discovery = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = discovery
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let selected = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let selected_input =
        PreparedNavigationInput::from_source(&selected, &bundle, TILE, || false).unwrap();
    let historical = selected_input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let expanded = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[
            benilla_native_terrain::Tile { x: 31, y: 30 },
            benilla_native_terrain::Tile { x: 31, y: 29 },
        ],
    )
    .unwrap();
    let input = PreparedNavigationInput::from_source(&expanded, &bundle, TILE, || false).unwrap();
    assert_eq!(selected_input.summary().unwrap().tiles, 1);
    assert_eq!(input.summary().unwrap().tiles, 2);
    assert_ne!(
        selected_input.summary().unwrap().revision,
        input.summary().unwrap().revision
    );
    let original_map = oracle("ridge_xminus_wmo/000.mmap");
    let original_tile = oracle("ridge_xminus_wmo/0003130.mmtile");
    let manifest: serde_json::Value = serde_json::from_slice(&oracle("manifest.json")).unwrap();
    input.assert_original_products_for_test(
        &helper(),
        manifest["cases"]["ridge_xminus_wmo"]["helper_request"]["sha256"]
            .as_str()
            .unwrap(),
        &original_map,
        &original_tile,
    );
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    assert_navigation_bytes(output.products().0, &original_map, "original MMap");
    assert_navigation_bytes(output.products().1, &original_tile, "original MMTile");
    assert_eq!(&historical.products().0[..12], &output.products().0[..12]);
    assert_eq!(
        u32::from_le_bytes(historical.products().0[20..24].try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(output.products().0[20..24].try_into().unwrap()),
        2
    );
    assert_ne!(historical.revision(), output.revision());
    assert_ne!(historical.products().1, output.products().1);
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_xminus_wmo/expected.json")).unwrap();
    assert_original_routes(&mut output.load().unwrap(), &expected);
    assert_navigation_bytes(
        historical.products().0,
        &oracle("ridge_wmo/000.mmap"),
        "ridge_wmo/000.mmap",
    );
    assert_navigation_bytes(
        historical.products().1,
        &oracle("ridge_wmo/0003130.mmtile"),
        "ridge_wmo/0003130.mmtile",
    );
    historical.load().unwrap();
    // A genuine source Exact625 outcome stays unavailable, but differs from omission.
    use super::super::super::super::source_pathfinder_query::tests as provider;
    use super::super::super::source_query::SourceMapQuerySnapshot;
    let neighbor_input = PreparedNavigationInput::from_source(
        &expanded,
        &bundle,
        super::super::super::TileId { x: 31, y: 29 },
        || false,
    )
    .unwrap();
    let skipped = neighbor_input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    assert_eq!(
        skipped.summary().status,
        benilla_native_nav_generator::GenerationStatus::Exact625
    );
    let p = crossing_profile();
    positive_tile_load_tests::assert_inventory_refusals(
        &selected_input,
        &historical,
        &input,
        &output,
        &neighbor_input,
        &skipped,
        &p,
    );
    let omitted = SourceMapQuerySnapshot::from_generated(
        &[(&input, &output)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    let explicit_skip = SourceMapQuerySnapshot::from_generated(
        &[(&input, &output), (&neighbor_input, &skipped)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    assert_ne!(
        omitted.binding().inventory,
        explicit_skip.binding().inventory
    );
    provider::assert_generated_dispatch(p.clone(), omitted, None, false);
    provider::assert_generated_dispatch(p.clone(), explicit_skip, None, false);
    assert!(SourceMapQuerySnapshot::from_generated(
        &[(&neighbor_input, &skipped)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false
    )
    .is_err());
    assert!(SourceMapQuerySnapshot::from_generated(
        &[(&selected_input, &historical), (&neighbor_input, &skipped)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false
    )
    .is_err());
    selected_input.verify_unchanged(|| false).unwrap();
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_hole_neighbor_generates_two_tiles_and_crosses_the_source_seam() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let entries = cardinal_entries(oracle(
        "two-tile-seam/authored/ridge_xminus_hole.adt.test-fixture",
    ));
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[
            benilla_native_terrain::Tile { x: 31, y: 30 },
            benilla_native_terrain::Tile { x: 31, y: 29 },
        ],
    )
    .unwrap();
    assert_eq!(
        terrain
            .tiles()
            .find(|(tile, _)| tile.x == 31 && tile.y == 29)
            .unwrap()
            .1,
        oracle("two-tile-seam/authored/ridge_xminus_hole.map")
    );
    let discovery = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = discovery
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&oracle("two-tile-seam/manifest.json")).unwrap();
    let original_map = oracle("two-tile-seam/000.mmap");
    let mut outputs = Vec::new();
    for (name, grid, filename) in [
        ("selected", TILE, "0003130.mmtile"),
        (
            "neighbor",
            super::super::super::TileId { x: 31, y: 29 },
            "0003129.mmtile",
        ),
    ] {
        let input =
            PreparedNavigationInput::from_source(&terrain, &bundle, grid, || false).unwrap();
        let original_tile = oracle(&format!("two-tile-seam/{filename}"));
        input.assert_original_products_for_test(
            &helper(),
            manifest["tiles"][name]["helper_request"]["sha256"]
                .as_str()
                .unwrap(),
            &original_map,
            &original_tile,
        );
        let output = input
            .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
                false
            })
            .unwrap();
        assert_navigation_bytes(output.products().0, &original_map, "original MMap");
        assert_navigation_bytes(output.products().1, &original_tile, "original MMTile");
        input.verify_unchanged(|| false).unwrap();
        outputs.push(output);
    }
    let mut mesh = benilla_native_nav::Mesh::load(
        outputs[0].products().0,
        &[
            benilla_native_nav::TileInput {
                grid: [31, 30],
                bytes: outputs[0].products().1,
            },
            benilla_native_nav::TileInput {
                grid: [31, 29],
                bytes: outputs[1].products().1,
            },
        ],
    )
    .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("two-tile-seam/expected.json")).unwrap();
    let queries = expected["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 6);
    for query in queries {
        let point = |name: &str| {
            let v: Vec<u32> = query[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u32::from_str_radix(v.as_str().unwrap(), 16).unwrap())
                .collect();
            [v[2], v[0], v[1]]
        };
        let route = mesh
            .route(
                point("start_bits"),
                point("end_bits"),
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(route.start.polygon, query["start_ref"].as_u64().unwrap());
        assert_eq!(route.end.polygon, query["end_ref"].as_u64().unwrap());
        assert_eq!(
            route.polygons,
            query["corridor"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect::<Vec<_>>()
        );
        let tile_refs: std::collections::BTreeSet<_> = query["corridor_tile_refs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(tile_refs.len(), 2);
        assert_eq!(route.polygons.last(), Some(&route.end.polygon));
    }
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_generated_tiles_connect_to_actual_owner_constructor_query() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::tests as provider;
    use super::super::super::source_query::SourceMapQuerySnapshot;
    let entries = cardinal_entries(oracle(
        "two-tile-seam/authored/ridge_xminus_hole.adt.test-fixture",
    ));
    let install = Install::new(&entries);
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[
            benilla_native_terrain::Tile { x: 31, y: 30 },
            benilla_native_terrain::Tile { x: 31, y: 29 },
        ],
    )
    .unwrap();
    let discovery = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = discovery
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let inputs = [
        PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap(),
        PreparedNavigationInput::from_source(
            &terrain,
            &bundle,
            super::super::super::TileId { x: 31, y: 29 },
            || false,
        )
        .unwrap(),
    ];
    let outputs: Vec<_> = inputs
        .iter()
        .map(|input| {
            input
                .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
                    false
                })
                .unwrap()
        })
        .collect();
    let p = crossing_profile();
    let pairs = [(&inputs[0], &outputs[0]), (&inputs[1], &outputs[1])];
    for invalid in provider::generated_rejected_configurations(&p) {
        assert!(SourceMapQuerySnapshot::from_generated(
            &pairs,
            &invalid,
            Instant::now() + Duration::from_secs(10),
            || false
        )
        .is_err());
    }

    assert!(matches!(
        SourceMapQuerySnapshot::from_generated(
            &[],
            &p,
            Instant::now() + Duration::from_secs(10),
            || false
        ),
        Err(GenerationError::Validation(
            super::super::super::Error::Missing
        ))
    ));
    assert!(matches!(
        SourceMapQuerySnapshot::from_generated(
            &[pairs[0]; 10],
            &p,
            Instant::now() + Duration::from_secs(10),
            || false
        ),
        Err(GenerationError::Validation(
            super::super::super::Error::Budget
        ))
    ));
    assert!(matches!(
        SourceMapQuerySnapshot::from_generated(
            &[pairs[0], pairs[0]],
            &p,
            Instant::now() + Duration::from_secs(10),
            || false
        ),
        Err(GenerationError::Validation(
            super::super::super::Error::Duplicate
        ))
    ));
    assert!(matches!(
        SourceMapQuerySnapshot::from_generated(
            &[(&inputs[0], &outputs[1])],
            &p,
            Instant::now() + Duration::from_secs(10),
            || false
        ),
        Err(GenerationError::Validation(
            super::super::super::Error::Identity
        ))
    ));
    assert!(matches!(
        SourceMapQuerySnapshot::from_generated(
            &pairs,
            &p,
            Instant::now() + Duration::from_secs(10),
            || true
        ),
        Err(GenerationError::Cancelled)
    ));
    assert!(matches!(
        SourceMapQuerySnapshot::from_generated(&pairs, &p, Instant::now(), || false),
        Err(GenerationError::Deadline)
    ));
    let snapshot = SourceMapQuerySnapshot::from_generated(
        &pairs,
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    let replacement = SourceMapQuerySnapshot::from_generated(
        &pairs,
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    let reversed = SourceMapQuerySnapshot::from_generated(
        &[pairs[1], pairs[0]],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    assert_eq!(snapshot.binding(), reversed.binding());
    provider::assert_generated_binding_rejection(p.clone(), reversed);
    provider::assert_generated_dispatch(p.clone(), snapshot, Some(replacement), true);
    let selected_only = SourceMapQuerySnapshot::from_generated(
        &pairs[..1],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    provider::assert_generated_dispatch(p.clone(), selected_only, None, false);
    let polls = std::cell::Cell::new(0usize);
    let historical = SourceMapQuerySnapshot::from_generated(
        &pairs,
        &p,
        Instant::now() + Duration::from_secs(10),
        || {
            polls.set(polls.get() + 1);
            false
        },
    )
    .unwrap();
    let count = polls.get();
    assert!(count > 10);
    polls.set(0);
    assert!(SourceMapQuerySnapshot::from_generated(
        &pairs,
        &p,
        Instant::now() + Duration::from_secs(10),
        || {
            polls.set(polls.get() + 1);
            polls.get() == count
        }
    )
    .is_err());
    let reduced_terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let different_input =
        PreparedNavigationInput::from_source(&reduced_terrain, &bundle, TILE, || false).unwrap();
    assert!(matches!(
        SourceMapQuerySnapshot::from_generated(
            &[(&different_input, &outputs[0])],
            &p,
            Instant::now() + Duration::from_secs(10),
            || false
        ),
        Err(GenerationError::Validation(
            super::super::super::Error::Identity
        ))
    ));
    // Historical native query remains owned, while a new construction must recheck source.
    let mut changed = entries;
    let adt = changed.iter_mut().find(|e| e.name == ADT).unwrap();
    adt.bytes[0] ^= 1;
    super::super::super::nav_generation::assert_source_change_after_query_load(&pairs, &p, || {
        install.write(&changed)
    });
    assert!(SourceMapQuerySnapshot::from_generated(
        &pairs,
        &p,
        Instant::now() + Duration::from_secs(10),
        || false
    )
    .is_err());
    provider::assert_generated_dispatch(p, historical, None, true);
}

fn crossing_profile() -> benilla_world_runtime::native::Profile {
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("two-tile-seam/expected.json")).unwrap();
    let query = &expected["queries"][0];
    let point = |name: &str| {
        let values: Vec<_> = query[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| f32::from_bits(u32::from_str_radix(v.as_str().unwrap(), 16).unwrap()))
            .collect();
        [values[2], values[0], values[1]]
    };
    let mut start = point("start_bits");
    let mut end = point("end_bits");
    // Constructor/tile-presence evidence only: use a ten-unit aggro pair straddling
    // the same seam, rather than the original 178-unit route-query endpoints.
    let seam = (start[1] + end[1]) * 0.5;
    start[1] = seam - 5.0;
    end[1] = seam + 5.0;
    super::super::super::super::source_pathfinder_query::tests::generated_profile(start, end)
}

#[test]
fn genuine_source_points_feed_actual_constructor_filter_and_expire() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::tests as provider;
    use super::super::super::{
        source_height::PreparedSourceHeight, source_query::SourceMapQuerySnapshot,
    };
    let entries = cardinal_entries(oracle(
        "two-tile-seam/authored/ridge_xminus_hole.adt.test-fixture",
    ));
    let install = Install::new(&entries);
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[
            benilla_native_terrain::Tile { x: 31, y: 30 },
            benilla_native_terrain::Tile { x: 31, y: 29 },
        ],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let p = crossing_profile();
    let query = SourceMapQuerySnapshot::from_generated(
        &[(&input, &output)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    provider::assert_resident_filter(p, query, &height, || {
        let mut changed = entries;
        changed.iter_mut().find(|e| e.name == ADT).unwrap().bytes[0] ^= 1;
        install.write(&changed);
    });
}

#[test]
fn genuine_generated_resident_cold_same_polygon_is_private_fresh_and_retryable() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::{
        resident_cold_tests as resident, tests as provider,
    };
    use super::super::super::nav_generation::fresh_grid::{FreshSingleGrid, GridConfiguration};
    let entries = ridge_entries();
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    assert_navigation_bytes(
        output.products().0,
        &oracle("ridge_wmo/000.mmap"),
        "ridge_wmo/000.mmap",
    );
    assert_navigation_bytes(
        output.products().1,
        &oracle("ridge_wmo/0003130.mmtile"),
        "ridge_wmo/0003130.mmtile",
    );

    // Existing original scalar-query start, converted from Detour YZX to source
    // XYZ. Its first ground polygon is 21.333 units wide along source Y; the
    // eight-unit aggro pair stays within it. Verify on the generated bytes before
    // involving the real runtime's near-point and constructor dispatch.
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let source = &expected["output"]["paths"][0];
    let values: Vec<_> = source["start_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| f32::from_bits(u32::from_str_radix(v.as_str().unwrap(), 16).unwrap()))
        .collect();
    let start = [values[2], values[0], values[1]];
    let end = [start[0], start[1] + 8.0, start[2]];
    let scalar = output
        .load()
        .unwrap()
        .route(
            start.map(f32::to_bits),
            end.map(f32::to_bits),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(scalar.polygons, [source["start_ref"].as_u64().unwrap()]);
    assert_eq!(scalar.start.polygon, scalar.end.polygon);
    let p = provider::generated_profile(start, end);
    // Source grid creation is requested by actual source positions. The grid,
    // actual height and exact query lease remain alive throughout runtime use.
    let position = [start[0].to_bits(), start[1].to_bits()];
    let load = || {
        FreshSingleGrid::from_generated(
            &input,
            &output,
            &p,
            position,
            Some(GridConfiguration {
                expiry_ms: 300_000,
                unload_enabled: true,
                vmap: positive_tile_load_tests::ENABLED,
            }),
            Instant::now() + Duration::from_secs(10),
            || false,
        )
        .unwrap()
    };
    let owner = load();
    let replacement = load();
    owner
        .ensure(
            [end[0].to_bits(), end[1].to_bits()],
            Instant::now() + Duration::from_secs(10),
            || false,
        )
        .unwrap();
    assert_eq!(owner.references(), 1);
    let mut changed = ridge_entries();
    changed.iter_mut().find(|e| e.name == ADT).unwrap().bytes[0] ^= 1;
    owner
        .with_query(|grid, query| {
            assert_eq!(grid.linked_count(), 1);
            grid.check_query(&query).unwrap();
            replacement
                .with_query(|other, replacement_query| {
                    assert!(grid.check_query(&replacement_query).is_err());
                    other.check_query(&replacement_query).unwrap();
                    resident::assert_resident_cold_terminal(
                        p,
                        query,
                        replacement_query,
                        grid.height(),
                        |mutated| {
                            install.write(if mutated { &changed } else { &entries });
                        },
                    );
                    assert_eq!(grid.linked_count(), 1);
                    assert_eq!(other.linked_count(), 1);
                    assert!(!grid.object_data_loaded());
                })
                .unwrap();
        })
        .unwrap();
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_resident_cold_calculate_missing_endpoint_precedes_liquid_update() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::resident_cold_tests as resident;
    use super::super::super::{
        source_height::PreparedSourceHeight, source_query::SourceMapQuerySnapshot,
    };
    let entries = cardinal_entries(oracle(
        "two-tile-seam/authored/ridge_xminus_hole.adt.test-fixture",
    ));
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[
            benilla_native_terrain::Tile { x: 31, y: 30 },
            benilla_native_terrain::Tile { x: 31, y: 29 },
        ],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let p = crossing_profile();
    let query = SourceMapQuerySnapshot::from_generated(
        &[(&input, &output)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    resident::assert_resident_missing_tile_short_circuits(p, query, &height);
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_resident_cold_calculate_different_polygons_have_no_terminal() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::{
        resident_cold_tests as resident, tests as provider,
    };
    use super::super::super::{
        source_height::PreparedSourceHeight, source_query::SourceMapQuerySnapshot,
    };
    let install = Install::new(&ridge_entries());
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let source = &expected["output"]["paths"][0];
    let values: Vec<_> = source["start_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| f32::from_bits(u32::from_str_radix(v.as_str().unwrap(), 16).unwrap()))
        .collect();
    let start = [values[2], values[0], values[1]];
    // Cross the first polygon's nearer source-Y edge while remaining inside
    // the generated tile and the ordinary ten-unit aggro/contact neighborhood.
    let end = [start[0], start[1] - 10.0, start[2]];
    let scalar = output
        .load()
        .unwrap()
        .route(
            start.map(f32::to_bits),
            end.map(f32::to_bits),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(scalar.start.polygon, source["start_ref"].as_u64().unwrap());
    assert_ne!(scalar.start.polygon, scalar.end.polygon);
    assert!(scalar.polygons.len() > 1);
    let p = provider::generated_profile(start, end);
    let query = SourceMapQuerySnapshot::from_generated(
        &[(&input, &output)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    resident::assert_resident_different_polygon_rejected(p, query, &height);
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_resident_chase_los_keeps_model_witnesses_fresh_and_exact_goal_endpoints() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::{
        resident_los_tests as resident, tests as provider,
    };
    use super::super::super::super::source_route_filter::resident::chase_los::ResidentChaseLosDecision as Decision;
    use super::super::super::{
        source_height::PreparedSourceHeight, source_query::SourceMapQuerySnapshot,
    };
    let entries = ridge_entries();
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let source = &expected["output"]["paths"][0];
    let values: Vec<_> = source["start_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| f32::from_bits(u32::from_str_radix(v.as_str().unwrap(), 16).unwrap()))
        .collect();
    let start = [values[2], values[0], values[1]];
    let end = [start[0], start[1] + 8.0, start[2]];
    let p = resident::model_profile(start, end, 1.6, 2.4);
    let snapshot = |p: &benilla_world_runtime::native::Profile| {
        SourceMapQuerySnapshot::from_generated(
            &[(&input, &output)],
            p,
            Instant::now() + Duration::from_secs(10),
            || false,
        )
        .unwrap()
    };
    let query = snapshot(&p);
    let replacement = snapshot(&p);
    let mut changed = ridge_entries();
    changed.iter_mut().find(|e| e.name == ADT).unwrap().bytes[0] ^= 1;
    resident::assert_resident_chase_los(p.clone(), query, replacement, &height, |mutated| {
        install.write(if mutated { &changed } else { &entries });
    });
    // Strict 200+selected-owner-reach distance boundary precedes the strict
    // five-unit vertical boundary, with no LOS request after either rejection.
    let reach = benilla_world_runtime::native::synthetic_creature_model_fixture::selection()
        .combat_reach()
        .bits();
    for (goal, decision) in [
        (
            [200.0 + f32::from_bits(reach), 0.0, 0.0],
            Decision::Distance,
        ),
        // Downward subtraction keeps this fixture's five-unit boundary exact;
        // upward addition at its nonintegral Z can round just below the boundary.
        ([0.0, 0.0, -5.0], Decision::Vertical),
        ([500.0, 0.0, -5.0], Decision::Distance),
    ] {
        resident::assert_resident_los_decision(p.clone(), snapshot(&p), &height, goal, decision);
    }
    let scalar = provider::generated_profile(start, end);
    let query = snapshot(&scalar);
    resident::assert_resident_scalar_body_is_not_model(scalar, query, &height);

    // Original generated Hull's pinned hit XY and z=5 plane. Raw authored model
    // fields produce different eyes across that plane while the feet pass both
    // numeric predicates; the real runtime seals this authored near-point goal.
    let hit = [
        f32::from_bits(0x43e8_d540),
        f32::from_bits(0x4471_aaa0),
        0.0,
    ];
    let player = [hit[0] + 10.0, hit[1], hit[2]];
    let blocked = resident::model_profile(hit, player, 1.0, 16.0);
    resident::assert_resident_los_decision(
        blocked.clone(),
        snapshot(&blocked),
        &height,
        [hit[0] + 0.01, hit[1], hit[2]],
        Decision::Blocked,
    );
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_resident_chase_water_binds_catalog_then_reads_runtime_cached_target() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::{
        resident_los_tests as los, resident_water_tests as water,
    };
    use super::super::super::super::source_route_filter::resident::chase_los::ResidentChaseLosDecision as Decision;
    use super::super::super::{
        source_height::{CompositionError, PreparedSourceHeight},
        source_query::SourceMapQuerySnapshot,
    };
    use benilla_world_runtime::native::environment::LiquidCatalog;

    let mut entries = ridge_entries();
    let liquid = entries
        .iter_mut()
        .find(|entry| entry.name == "DBFilesClient\\LiquidType.dbc")
        .unwrap();
    // Keep the existing authored row and add an explicit spell-free surface row.
    // Both runtime catalog and source receipts consume these same complete bytes.
    assert_eq!(
        u32::from_le_bytes(liquid.bytes[4..8].try_into().unwrap()),
        1
    );
    liquid.bytes[4..8].copy_from_slice(&2u32.to_le_bytes());
    liquid.bytes[16..20].copy_from_slice(&8u32.to_le_bytes());
    liquid.bytes.truncate(36);
    let surface_entry = 22u32;
    for word in [surface_entry, 22, 3, 0] {
        liquid.bytes.extend(word.to_le_bytes());
    }
    liquid.bytes.extend(b"\0source\0");
    liquid.declared = liquid.bytes.len() as u32;
    let catalog_bytes = liquid.bytes.clone();
    let catalog = LiquidCatalog::from_wdbc(&catalog_bytes).unwrap();
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let start_bits = expected["output"]["paths"][0]["start_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| f32::from_bits(u32::from_str_radix(v.as_str().unwrap(), 16).unwrap()))
        .collect::<Vec<_>>();
    let start = [start_bits[2], start_bits[0], start_bits[1]];
    let end = [start[0], start[1] + 8.0, start[2]];
    let p = los::model_profile(start, end, 1.6, 2.4);
    let snapshot = |p: &benilla_world_runtime::native::Profile| {
        SourceMapQuerySnapshot::from_generated(
            &[(&input, &output)],
            p,
            Instant::now() + Duration::from_secs(10),
            || false,
        )
        .unwrap()
    };

    let binding = height
        .environment_binding(p.content_digest, &catalog, || false)
        .unwrap();
    assert_eq!(binding.map, p.map_id);
    assert_eq!(binding.content, p.content_digest);
    assert_eq!(binding.liquid_catalog, catalog.digest());
    assert_ne!(binding.source_epoch, [0; 32]);
    assert_ne!(binding.inventory, [0; 32]);
    assert_ne!(binding.policy, [0; 32]);
    assert_eq!(
        height
            .environment_binding(p.content_digest, &catalog, || false)
            .unwrap(),
        binding
    );
    let legacy = LiquidCatalog::from_legacy_wdbc(&catalog_bytes).unwrap();
    assert_eq!(legacy.digest(), catalog.digest());
    assert_eq!(legacy.get(surface_entry), catalog.get(surface_entry));
    assert!(height
        .environment_binding(p.content_digest, &legacy, || false)
        .is_err());
    let mut changed_bytes = catalog_bytes.clone();
    changed_bytes[53] = b'S';
    let changed_catalog = LiquidCatalog::from_wdbc(&changed_bytes).unwrap();
    assert_eq!(changed_catalog.get(1), catalog.get(1));
    assert_eq!(
        changed_catalog.get(surface_entry),
        catalog.get(surface_entry)
    );
    assert_ne!(changed_catalog.digest(), catalog.digest());
    assert!(height
        .environment_binding(p.content_digest, &changed_catalog, || false)
        .is_err());
    assert!(height
        .environment_binding([0; 32], &catalog, || false)
        .is_err());
    assert!(matches!(
        height.environment_binding(p.content_digest, &catalog, || true),
        Err(CompositionError::Cancelled)
    ));

    let mut changed: Vec<_> = entries
        .iter()
        .map(|entry| Entry {
            name: entry.name.clone(),
            bytes: entry.bytes.clone(),
            flags: entry.flags,
            declared: entry.declared,
        })
        .collect();
    changed
        .iter_mut()
        .find(|entry| entry.name == ADT)
        .unwrap()
        .bytes[0] ^= 1;
    water::assert_resident_environment_reply(
        p.clone(),
        snapshot(&p),
        &height,
        &catalog,
        |mutated| install.write(if mutated { &changed } else { &entries }),
    );
    water::assert_resident_chase_water(
        p.clone(),
        snapshot(&p),
        snapshot(&p),
        &height,
        &catalog,
        |mutated| {
            install.write(if mutated { &changed } else { &entries });
        },
    );
    water::assert_resident_water_before_countdown(p.clone(), snapshot(&p), &height, &catalog);
    water::assert_resident_water_cached_target_is_wet(
        p.clone(),
        snapshot(&p),
        &height,
        &catalog,
        surface_entry,
    );
    for field in ["epoch", "inventory", "policy", "missing"] {
        water::assert_resident_water_binding_rejected(
            p.clone(),
            snapshot(&p),
            &height,
            &catalog,
            field,
        );
    }
    let reach = benilla_world_runtime::native::synthetic_creature_model_fixture::selection()
        .combat_reach()
        .bits();
    for (goal, decision) in [
        (
            [200.0 + f32::from_bits(reach), 0.0, 0.0],
            Decision::Distance,
        ),
        ([0.0, 0.0, -5.0], Decision::Vertical),
    ] {
        water::assert_resident_water_prefix_rejected(
            p.clone(),
            snapshot(&p),
            &height,
            goal,
            decision,
        );
    }
    let hit = [
        f32::from_bits(0x43e8_d540),
        f32::from_bits(0x4471_aaa0),
        0.0,
    ];
    let blocked = los::model_profile(hit, [hit[0] + 10.0, hit[1], hit[2]], 1.0, 16.0);
    water::assert_resident_water_prefix_rejected(
        blocked.clone(),
        snapshot(&blocked),
        &height,
        [hit[0] + 0.01, hit[1], hit[2]],
        Decision::Blocked,
    );
    install.write(&changed);
    assert!(height
        .environment_binding(p.content_digest, &catalog, || false)
        .is_err());
    install.write(&entries);
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_resident_player_environment_wet_and_missing_products_never_refresh_cache() {
    use super::super::super::super::source_pathfinder_query::{
        resident_los_tests as los, resident_water_tests as water,
    };
    use super::super::super::source_height::PreparedSourceHeight;
    use benilla_world_runtime::native::environment::LiquidCatalog;
    let terrain_fixture = |name: &str| {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../benilla-native-terrain/tools/fixtures")
                .join(name),
        )
        .unwrap()
    };
    let hit = [
        f32::from_bits(0x43e8_d540),
        f32::from_bits(0x4471_aaa0),
        8.0,
    ];
    for case in [
        "terrain_wet",
        "selected_wmo_wet",
        "missing_terrain",
        "missing_collision",
    ] {
        let mut entries = source_height_tests::shared_entries();
        match case {
            "terrain_wet" => {
                entries = placed_entries(terrain_fixture("liquid_water_flat.adt.test-fixture"));
            }
            "selected_wmo_wet" => {
                // Same original physical MLIQ chunk used by resident_liquid_tests;
                // this authored combination establishes refusal, not wet semantics.
                let wet = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../benilla-native-geometry-tests/tools/fixtures/wmo_liquid_source/inputs/Ocean_000.group.test-fixture"
                )).unwrap();
                let offset = wet.windows(4).position(|tag| tag == b"QILM").unwrap();
                let size =
                    u32::from_le_bytes(wet[offset + 4..offset + 8].try_into().unwrap()) as usize;
                let mut group = fixture("hull_000.group.test-fixture");
                group.extend_from_slice(&wet[offset..offset + 8 + size]);
                *entries
                    .iter_mut()
                    .find(|entry| entry.name == GROUP)
                    .unwrap() = raw_entry(GROUP, group);
            }
            "missing_collision" => {
                *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() =
                    raw_entry(ADT, terrain_fixture("flat.adt.test-fixture"));
                let mut other = chunk(b"MVER", &18u32.to_le_bytes());
                other.extend(chunk(b"MWMO", format!("{ROOT}\0").as_bytes()));
                other.extend(chunk(b"MODF", &fixture("hull.modf.test-fixture")));
                entries.push(raw_entry(
                    "World\\Maps\\Selected\\Selected_31_31.adt",
                    other,
                ));
            }
            "missing_terrain" => {}
            _ => unreachable!(),
        }
        let catalog = LiquidCatalog::from_wdbc(
            &entries
                .iter()
                .find(|entry| entry.name == "DBFilesClient\\LiquidType.dbc")
                .unwrap()
                .bytes,
        )
        .unwrap();
        let install = Install::new(&entries);
        let before = install.bytes();
        let terrain = benilla_native_terrain::prepare(
            &install.0,
            0,
            &[benilla_native_terrain::Tile { x: 31, y: 30 }],
        )
        .unwrap();
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        let mut player = hit;
        if case == "missing_terrain" {
            player.swap(0, 1);
        }
        let p = los::model_profile([player[0], player[1] - 8.0, player[2]], player, 1.6, 2.4);
        let error = if case.starts_with("missing") {
            Error::Missing
        } else {
            Error::Unsupported
        };
        water::assert_resident_environment_unavailable(p, &height, &catalog, error);
        height.verify_unchanged(|| false).unwrap();
        assert_eq!(install.bytes(), before, "{case}");
    }
}

#[test]
fn genuine_resident_primary_point_drives_runtime_dry_cache_and_conditional_chase() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::source_pathfinder_query::{
        resident_los_tests as los, resident_near_point_tests as primary,
    };
    use super::super::super::{
        source_height::PreparedSourceHeight, source_query::SourceMapQuerySnapshot,
    };
    use benilla_world_runtime::native::environment::LiquidCatalog;
    let entries = ridge_entries();
    let catalog = LiquidCatalog::from_wdbc(
        &entries
            .iter()
            .find(|entry| entry.name == "DBFilesClient\\LiquidType.dbc")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let start = expected["output"]["paths"][0]["start_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|word| f32::from_bits(u32::from_str_radix(word.as_str().unwrap(), 16).unwrap()))
        .collect::<Vec<_>>();
    let start = [start[2], start[0], start[1]];
    // Existing authored model and starter sources remain labeled. Every selected
    // NearPoint geometry observation comes from the real MPQ consumed inputs.
    let p = los::model_profile(start, [start[0], start[1] + 8.0, start[2]], 1.6, 2.4);
    let snapshot = || {
        SourceMapQuerySnapshot::from_generated(
            &[(&input, &output)],
            &p,
            Instant::now() + Duration::from_secs(10),
            || false,
        )
        .unwrap()
    };
    let mut changed = ridge_entries();
    changed
        .iter_mut()
        .find(|entry| entry.name == ADT)
        .unwrap()
        .bytes[0] ^= 1;
    primary::assert_resident_primary_near_point(
        p.clone(),
        snapshot(),
        snapshot(),
        snapshot(),
        &height,
        &catalog,
        |mutated| install.write(if mutated { &changed } else { &entries }),
    );
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_resident_primary_vertical_and_missing_geometry_never_publish_goals() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::{
        core::Failure,
        source_pathfinder_query::{
            resident_los_tests as los, resident_near_point_tests as primary,
        },
    };
    use super::super::super::{
        source_height::PreparedSourceHeight, source_query::SourceMapQuerySnapshot,
    };
    let entries = ridge_entries();
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    for (start, end, error, observations) in [
        // The supported ground candidate is well below the target. Vertical
        // rejection occurs after real ground and before any ray observation.
        (
            [88.0, 622.0, 30.0],
            [88.0, 630.0, 30.0],
            Failure::UnsupportedNearPointFallback,
            1,
        ),
        // These actual runtime poses select an entirely unloaded terrain tile.
        (
            [0.0, -8.0, 7.25],
            [0.0, 0.0, 7.25],
            Failure::MissingGeometry,
            0,
        ),
    ] {
        let p = los::model_profile(start, end, 1.6, 2.4);
        let query = SourceMapQuerySnapshot::from_generated(
            &[(&input, &output)],
            &p,
            Instant::now() + Duration::from_secs(10),
            || false,
        )
        .unwrap();
        primary::assert_resident_primary_unavailable(p, query, &height, error, observations);
    }
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

#[test]
fn genuine_resident_primary_blocked_los_never_publishes_a_goal() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    use super::super::super::super::{
        core::Failure,
        source_pathfinder_query::{
            resident_los_tests as los, resident_near_point_tests as primary,
        },
    };
    use super::super::super::{
        source_height::PreparedSourceHeight, source_query::SourceMapQuerySnapshot,
    };
    let entries = ridge_entries();
    let install = Install::new(&entries);
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    assert_eq!(
        output.summary().status,
        benilla_native_nav_generator::GenerationStatus::Generated
    );
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let hit = [f32::from_bits(0x43e8_d540), f32::from_bits(0x4471_aaa0)];
    // Source Hull rises above the target's eye and falls below candidate feet,
    // so the actual ray crosses the interior rather than an endpoint seam.
    let p = los::model_profile([hit[0] + 8.0, hit[1], 4.5], [hit[0], hit[1], 4.5], 0.1, 2.4);
    let query = SourceMapQuerySnapshot::from_generated(
        &[(&input, &output)],
        &p,
        Instant::now() + Duration::from_secs(10),
        || false,
    )
    .unwrap();
    primary::assert_resident_primary_unavailable(
        p,
        query,
        &height,
        Failure::UnsupportedNearPointFallback,
        2,
    );
    // Missing collision-slot fallback remains separate primitive coverage in
    // source_height_tests; this fixture proves no loaded resident composition
    // for that case. Missing terrain is exercised through NativeWorld above.
    input.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}
