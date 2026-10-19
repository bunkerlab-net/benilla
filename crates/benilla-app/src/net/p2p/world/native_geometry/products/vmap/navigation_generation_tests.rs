// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine authored MPQ capture through generation and the production Detour reader.
use super::super::super::nav_generation::{GenerationError, PreparedNavigationInput};
use super::*;
use std::time::{Duration, Instant};
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
    assert_eq!(params, oracle("ridge_wmo/000.mmap"));
    assert_eq!(tile, oracle("ridge_wmo/0003130.mmtile"));
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
    assert_eq!(
        expanded.products().0,
        oracle("ridge_neighbors_wmo/000.mmap")
    );
    assert_eq!(
        expanded.products().1,
        oracle("ridge_neighbors_wmo/0003130.mmtile")
    );
    assert_eq!(old.products().0, oracle("ridge_wmo/000.mmap"));
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
    assert_eq!(
        output.products(),
        (original_map.as_slice(), original_tile.as_slice())
    );
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
    assert_eq!(historical.products().0, oracle("ridge_wmo/000.mmap"));
    assert_eq!(historical.products().1, oracle("ridge_wmo/0003130.mmtile"));
    historical.load().unwrap();
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
        assert_eq!(
            output.products(),
            (original_map.as_slice(), original_tile.as_slice())
        );
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
