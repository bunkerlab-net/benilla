// SPDX-License-Identifier: GPL-3.0-or-later
//! Run the actual helper over authored MPQs; successful inspection must never publish.
#[path = "support/map_dbc.rs"]
mod map_fixture;
mod support;
use benilla_native_terrain::{prepare, Tile};
use map_fixture::map_dbc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
use support::{archive, raw_entry, version_one_archive, Entry};
struct Fixture {
    root: PathBuf,
    data: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "benilla-inspect-private-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let data = root.join("installation/Data");
        fs::create_dir_all(&data).unwrap();
        Self { root, data }
    }
    fn install(&self, case: &str) -> Vec<u8> {
        self.install_adt(source_fixture(&format!("{case}.adt.test-fixture")))
    }
    fn install_adt(&self, adt: Vec<u8>) -> Vec<u8> {
        let bytes = archive(&terrain_entries(adt));
        fs::write(self.data.join("terrain.MPQ"), &bytes).unwrap();
        bytes
    }
    fn run(&self, args: &[&str]) -> Output {
        self.run_selection(1, Tile { x: 31, y: 32 }, args)
    }
    fn run_selection(&self, map: u32, tile: Tile, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_benilla-native-terrain"))
            .current_dir(&self.root)
            .arg("--data")
            .arg(&self.data)
            .arg("--map")
            .arg(map.to_string())
            .arg("--tile")
            .arg(format!("{},{}", tile.x, tile.y))
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn terrain_entries(adt: Vec<u8>) -> [Entry; 5] {
    [
        raw_entry(
            "DBFilesClient\\Map.dbc",
            include_bytes!("../tools/fixtures/Map.dbc.test-fixture").to_vec(),
        ),
        raw_entry(
            "DBFilesClient\\AreaTable.dbc",
            include_bytes!("../tools/fixtures/AreaTable.dbc.test-fixture").to_vec(),
        ),
        raw_entry(
            "DBFilesClient\\LiquidType.dbc",
            include_bytes!("../tools/fixtures/LiquidType.dbc.test-fixture").to_vec(),
        ),
        raw_entry(
            "World\\Maps\\AuthoredOracle\\AuthoredOracle.wdt",
            include_bytes!("../tools/fixtures/source.wdt.test-fixture").to_vec(),
        ),
        raw_entry("World\\Maps\\AuthoredOracle\\AuthoredOracle_32_31.adt", adt),
    ]
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, at: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            let dir = path.is_dir();
            out.insert(
                path.strip_prefix(root).unwrap().to_path_buf(),
                if dir {
                    None
                } else {
                    Some(fs::read(&path).unwrap())
                },
            );
            if dir {
                visit(root, &path, out);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
fn source_fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tools/fixtures")
            .join(name),
    )
    .unwrap()
}

const DIAGNOSTIC_STAGES: [&str; 8] = [
    "selection",
    "initial_capture",
    "map_table",
    "selection_capture",
    "full_capture",
    "area_table",
    "liquid_table",
    "source_recheck",
];
const TILE_DIAGNOSTIC_STAGES: [&str; 5] = [
    "wdt_selection",
    "adt_structure",
    "liquid",
    "history_resolution",
    "production",
];

fn assert_keys(value: &Value, expected: &[&str]) {
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    let mut actual: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    actual.sort_unstable();
    assert_eq!(actual, expected, "{value}");
}

fn assert_stage(stages: &Value, name: &str, status: &str, diagnostic_id: Option<&str>) {
    assert_eq!(
        stages[name],
        json!({"status": status, "diagnostic_id": diagnostic_id}),
        "{name}: {stages}"
    );
}

fn assert_sha256(value: &Value) {
    let digest = value.as_str().unwrap();
    assert_eq!(digest.len(), 64);
    assert!(digest
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
}

fn diagnostic_report(fixture: &Fixture, map: u32, tile: Tile, args: &[&str]) -> Value {
    let before = snapshot(&fixture.root);
    let output = fixture.run_selection(map, tile, args);
    assert_eq!(output.status.code(), Some(0), "{}", output_text(&output));
    assert!(output.stderr.is_empty(), "{}", output_text(&output));
    let text = std::str::from_utf8(&output.stdout).unwrap();
    // Parsing the complete stdout forbids banners, trailing reports and raw diagnostics.
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(text.len() < 16 * 1024);
    for forbidden in [
        "/",
        "\\",
        "private",
        "SECRET",
        "AuthoredOracle",
        "Azeroth",
        "Prototype",
        "DBFilesClient",
        ".MPQ",
        "installation",
    ] {
        assert!(!text.contains(forbidden), "unexpected {forbidden}: {text}");
    }
    assert_keys(
        &report,
        &[
            "format",
            "version",
            "map",
            "map_table",
            "source_epoch",
            "live_install_lease",
            "gameplay_admitted",
            "stages",
            "tiles",
        ],
    );
    assert_eq!(report["format"], "benilla-terrain-diagnostic");
    assert_eq!(report["version"], 1);
    assert_eq!(report["map"], map);
    assert_eq!(report["live_install_lease"], false);
    assert_eq!(report["gameplay_admitted"], false);
    if !report["source_epoch"].is_null() {
        assert_sha256(&report["source_epoch"]);
    }
    if !report["map_table"].is_null() {
        assert_keys(
            &report["map_table"],
            &["selected_row_ordinal", "total_rows"],
        );
    }
    assert_keys(&report["stages"], &DIAGNOSTIC_STAGES);
    let tiles = report["tiles"].as_array().unwrap();
    assert!(tiles.len() <= 9);
    for tile in tiles {
        assert_keys(
            tile,
            &[
                "x",
                "y",
                "raw_adt",
                "active_mclq_cells",
                "liquid_rectangle",
                "missing_liquid_heights",
                "history",
                "stages",
            ],
        );
        assert_keys(&tile["stages"], &TILE_DIAGNOSTIC_STAGES);
        if !tile["raw_adt"].is_null() {
            assert_keys(&tile["raw_adt"], &["bytes", "sha256"]);
            assert!(tile["raw_adt"]["bytes"].is_u64());
            assert_sha256(&tile["raw_adt"]["sha256"]);
        }
        if !tile["liquid_rectangle"].is_null() {
            assert_keys(&tile["liquid_rectangle"], &["x", "y", "width", "height"]);
        }
        if !tile["history"].is_null() {
            assert_keys(
                &tile["history"],
                &[
                    "consulted_adts",
                    "consulted_wdts",
                    "absent_members",
                    "initial_zero_samples",
                ],
            );
            for count in tile["history"].as_object().unwrap().values() {
                assert!(count.is_u64());
            }
        }
        if !tile["missing_liquid_heights"].is_null() {
            assert_keys(
                &tile["missing_liquid_heights"],
                &["count", "first_x", "first_y"],
            );
        }
    }
    for stages in std::iter::once(&report["stages"]).chain(tiles.iter().map(|t| &t["stages"])) {
        for stage in stages.as_object().unwrap().values() {
            assert_keys(stage, &["status", "diagnostic_id"]);
            match stage["status"].as_str().unwrap() {
                "passed" | "unavailable" => assert!(stage["diagnostic_id"].is_null()),
                "failed" => assert!(
                    [
                        "selection.bounds",
                        "selection.tile_bounds",
                        "selection.duplicate_tile",
                        "source.data_directory_required",
                        "source.ad_header_signature",
                        "terrain.AreaTable",
                        "terrain.LiquidType",
                        "terrain.adt_version",
                        "terrain.liquid_history",
                        "terrain.map_directory",
                        "terrain.wdt_tile_absent",
                    ]
                    .contains(&stage["diagnostic_id"].as_str().unwrap()),
                    "{stage}"
                ),
                status => panic!("unexpected stage status: {status}"),
            }
        }
    }
    assert_eq!(snapshot(&fixture.root), before);
    report
}

fn assert_raw_adt(tile: &Value, raw: &[u8]) {
    assert_eq!(
        tile["raw_adt"],
        json!({
            "bytes": raw.len(), "sha256": format!("{:x}", Sha256::digest(raw)),
        })
    );
}

fn assert_no_missing_heights(tile: &Value) {
    assert_eq!(
        tile["missing_liquid_heights"],
        json!({
            "count": 0, "first_x": null, "first_y": null,
        })
    );
}
fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn replace(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn assert_rejected_without_writes(fixture: &Fixture, expected: &str) {
    assert_selection_rejected_without_writes(fixture, 1, Tile { x: 31, y: 32 }, expected);
}
fn assert_selection_rejected_without_writes(
    fixture: &Fixture,
    map: u32,
    tile: Tile,
    expected: &str,
) {
    let before = snapshot(&fixture.root);
    for args in [&["--inspect"][..], &["--output", "rejected-export"][..]] {
        let output = fixture.run_selection(map, tile, args);
        assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
        let text = output_text(&output);
        assert!(text.starts_with(&format!("error[{expected}]:")), "{text}");
        assert!(text.contains("no gameplay admission established"));
        assert!(output.stdout.is_empty());
        assert_eq!(text.lines().count(), 1);
        assert!(text.len() < 256);
        assert!(!text.contains('/'));
        assert!(!text.contains('\\'));
        assert!(!text.contains(&fixture.root.to_string_lossy().to_string()));
        assert!(!text.contains("AuthoredOracle"));
        assert!(!text.contains("Azeroth"));
        assert!(!text.contains("Prototype"));
        assert!(!text.contains("private"));
        assert_eq!(snapshot(&fixture.root), before);
    }
}

fn assert_exact_inspection_without_writes(fixture: &Fixture, expected: &[u8]) -> String {
    assert_exact_selection_without_writes(fixture, 1, Tile { x: 31, y: 32 }, expected)
}
fn assert_exact_selection_without_writes(
    fixture: &Fixture,
    map: u32,
    tile: Tile,
    expected: &[u8],
) -> String {
    let before = snapshot(&fixture.root);
    let prepared = prepare(&fixture.data, map, &[tile]).unwrap();
    assert_eq!(prepared.map_id(), map);
    assert_eq!(prepared.tiles().collect::<Vec<_>>(), [(tile, expected)]);
    prepared.verify_unchanged().unwrap();
    let epoch = prepared.source_epoch();
    let expected_report = format!(
        "Inspected 1 terrain tiles in memory; no artifacts published\n\
         map={map} source_epoch={epoch}\n\
         tile={},{} bytes={} sha256={:x}\n\
         live_install_lease=false gameplay_admitted=false\n\
         No live installation lease or gameplay admission established\n",
        tile.x,
        tile.y,
        expected.len(),
        Sha256::digest(expected)
    );
    for _ in 0..2 {
        let output = fixture.run_selection(map, tile, &["--inspect"]);
        assert!(output.status.success(), "{}", output_text(&output));
        assert_eq!(output.stdout, expected_report.as_bytes());
        assert!(output.stderr.is_empty());
        assert_eq!(snapshot(&fixture.root), before);
    }
    epoch.to_owned()
}

// Only the selected map has WDT/ADT members. Other names are authored metadata,
// never archive paths or a claim about rows in a player's installation.
fn selected_azeroth_entries(map: Vec<u8>) -> [Entry; 5] {
    let mut entries = terrain_entries(source_fixture("flat.adt.test-fixture"));
    entries[0] = raw_entry("DBFilesClient\\Map.dbc", map);
    let mut wdt = source_fixture("source.wdt.test-fixture");
    assert_eq!(&wdt[52..56], b"NIAM");
    replace(&mut wdt, 60 + (31 * 64 + 32) * 8, 0);
    replace(&mut wdt, 60 + (48 * 64 + 32) * 8, 1);
    entries[3] = raw_entry("World\\Maps\\Azeroth\\Azeroth.wdt", wdt);
    entries[4] = raw_entry(
        "World\\Maps\\Azeroth\\Azeroth_32_48.adt",
        source_fixture("flat.adt.test-fixture"),
    );
    entries
}

#[test]
fn inspect_map_zero_ignores_unselected_path_syntax_but_binds_every_dbc_byte() {
    let fixture = Fixture::new();
    let mut map = map_dbc(&[
        (1, b"Kalimdor"),
        (900, b""),
        (0, b"Azeroth"),
        (901, b"Prototype Zone"),
        (902, b"../Unused"),
        (903, b"D\\Unused"),
    ]);
    let path = fixture.data.join("terrain.MPQ");
    fs::write(
        &path,
        version_one_archive(&selected_azeroth_entries(map.clone())),
    )
    .unwrap();
    let tile = Tile { x: 48, y: 32 };
    let product = source_fixture("flat.map");
    let mut epoch = assert_exact_selection_without_writes(&fixture, 0, tile, &product);
    // An unselected row's unused word and then its directory bytes remain bound.
    for offset in [28, 20 + 6 * 168 + 1] {
        let captured = prepare(&fixture.data, 0, &[tile]).unwrap();
        map[offset] ^= 1;
        fs::write(
            &path,
            version_one_archive(&selected_azeroth_entries(map.clone())),
        )
        .unwrap();
        let before = snapshot(&fixture.root);
        assert!(format!("{:#}", captured.verify_unchanged().unwrap_err())
            .contains("source.bytes_changed"));
        assert_eq!(snapshot(&fixture.root), before);
        let changed = assert_exact_selection_without_writes(&fixture, 0, tile, &product);
        assert_ne!(changed, epoch);
        epoch = changed;
    }
}

#[test]
fn inspect_selected_map_rejections_are_specific_path_free_and_write_nothing() {
    let fixture = Fixture::new();
    let tile = Tile { x: 48, y: 32 };
    let mut cases = Vec::new();
    for name in [
        "",
        " ",
        "Prototype Zone",
        "../Unused",
        "D/Unused",
        "D\\Unused",
        ".",
    ] {
        cases.push((
            map_dbc(&[(0, name.as_bytes()), (1, b"")]),
            "terrain.map_directory",
        ));
    }
    cases.push((map_dbc(&[(1, b"Kalimdor")]), "terrain.map_missing"));
    cases.push((
        map_dbc(&[(0, b"Azeroth"), (0, b"Azeroth")]),
        "terrain.duplicate_map_id",
    ));
    cases.push((
        map_dbc(&[(0, b"Azeroth"), (1, b""), (1, b"")]),
        "terrain.duplicate_map_id",
    ));
    for (row, expected) in [
        (0, "terrain.selected_map_metadata"),
        (1, "terrain.unselected_map_metadata"),
    ] {
        for invalid in [&[0xff][..], &[b'A'; 64][..]] {
            let mut rows: [(u32, &[u8]); 2] = [(0, b"Azeroth"), (1, b"Kalimdor")];
            rows[row].1 = invalid;
            cases.push((map_dbc(&rows), expected));
        }
        for extra in [0, 1] {
            let mut bytes = map_dbc(&[(0, b"Azeroth"), (1, b"Kalimdor")]);
            let offset = word(&bytes, 16) + extra;
            replace(&mut bytes, 24 + row * 168, offset);
            cases.push((bytes, expected));
        }
    }
    for (map, diagnostic) in cases {
        fs::write(
            fixture.data.join("terrain.MPQ"),
            version_one_archive(&selected_azeroth_entries(map)),
        )
        .unwrap();
        assert_selection_rejected_without_writes(&fixture, 0, tile, diagnostic);
    }
}
#[test]
fn inspect_actual_preparation_is_repeatable_path_free_and_writes_nothing() {
    let fixture = Fixture::new();
    fixture.install("flat");
    let before = snapshot(&fixture.root);
    let expected = format!(
        "{:x}",
        Sha256::digest(include_bytes!("../tools/fixtures/flat.map"))
    );
    let first = fixture.run(&["--inspect"]);
    assert!(first.status.success(), "{}", output_text(&first));
    let text = output_text(&first);
    assert!(
        text.contains("source_epoch=") && text.contains("tile=31,32") && text.contains(&expected),
        "{text}"
    );
    assert!(text.contains("gameplay_admitted=false"));
    assert!(!text.contains(&fixture.root.to_string_lossy().to_string()));
    assert_eq!(snapshot(&fixture.root), before);
    let second = fixture.run(&["--inspect"]);
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(snapshot(&fixture.root), before);
}
#[test]
fn inspect_admitted_liquid_is_repeatable_path_free_and_writes_nothing() {
    for case in [
        "liquid_water_flat",
        "liquid_ocean_flat",
        "liquid_magma_flat",
        "liquid_zero_size",
        "liquid_full_types",
        "liquid_flag_bytes",
        "liquid_deep_only",
        "liquid_nonsymmetric",
        "liquid_seams",
        "liquid_outer_edges",
        "liquid_sparse_flat",
    ] {
        let fixture = Fixture::new();
        fixture.install(case);
        let before = snapshot(&fixture.root);
        let expected_bytes = source_fixture(&format!("{case}.map"));
        assert_ne!(word(&expected_bytes, 24), 0, "{case}");
        let expected_hash = format!("{:x}", Sha256::digest(&expected_bytes));
        let first = fixture.run(&["--inspect"]);
        assert!(first.status.success(), "{case}: {}", output_text(&first));
        let text = output_text(&first);
        assert!(text.contains(&expected_hash), "{case}: {text}");
        assert!(text.contains(&format!("bytes={}", expected_bytes.len())));
        assert!(text.contains("source_epoch=") && text.contains("tile=31,32"));
        assert!(text.contains("live_install_lease=false gameplay_admitted=false"));
        assert!(!text.contains("dry terrain"));
        assert!(!text.contains(&fixture.root.to_string_lossy().to_string()));
        assert!(!text.contains("AuthoredOracle"));
        assert_eq!(snapshot(&fixture.root), before, "{case}");
        let second = fixture.run(&["--inspect"]);
        assert!(second.status.success(), "{case}: {}", output_text(&second));
        assert_eq!(first.stdout, second.stdout, "{case}");
        assert_eq!(first.stderr, second.stderr, "{case}");
        assert_eq!(snapshot(&fixture.root), before, "{case}");
    }
}

#[test]
fn inspect_source_skips_absent_or_empty_mclq_instead_of_inferring_water_from_flags() {
    let dry = source_fixture("flat.map");
    for case in [
        "liquid_absent_flags",
        "liquid_empty",
        "liquid_size8",
        "liquid_untyped",
    ] {
        let fixture = Fixture::new();
        fixture.install(case);
        let before = snapshot(&fixture.root);
        let expected = source_fixture(&format!("{case}.map"));
        assert_eq!(expected, dry, "{case}");
        let output = fixture.run(&["--inspect"]);
        assert!(output.status.success(), "{case}: {}", output_text(&output));
        let text = output_text(&output);
        assert!(text.contains(&format!("{:x}", Sha256::digest(&dry))));
        assert!(!text.contains(&fixture.root.to_string_lossy().to_string()));
        assert_eq!(snapshot(&fixture.root), before, "{case}");
    }
}

#[test]
fn inspect_closes_history_dependent_liquid_against_actual_predecessor_members() {
    for (case, seed) in [
        ("liquid_stale_edge_a", "liquid_water_flat"),
        ("liquid_stale_edge_b", "liquid_ocean_flat"),
    ] {
        let fixture = Fixture::new();
        install_history(&fixture, case, &[seed]);
        assert_exact_inspection_without_writes(&fixture, &source_fixture(&format!("{case}.map")));
    }
}

#[test]
fn inspect_refuses_malformed_liquid_without_leaking_paths_or_writing() {
    for (field, value, expected) in [
        (0x60, u32::MAX, "terrain.subchunk_reference"),
        (0x64, u32::MAX, "terrain.mclq_size"),
    ] {
        let fixture = Fixture::new();
        let mut adt = source_fixture("liquid_water_flat.adt.test-fixture");
        let cell = word(&adt, 92) as usize;
        replace(&mut adt, cell + 8 + field, value);
        fixture.install_adt(adt);
        assert_rejected_without_writes(&fixture, expected);
    }
    let fixture = Fixture::new();
    let mut adt = source_fixture("liquid_water_flat.adt.test-fixture");
    let cell = word(&adt, 92) as usize;
    let liquid = cell + word(&adt, cell + 8 + 0x60) as usize;
    replace(&mut adt, liquid + 4, 803);
    fixture.install_adt(adt);
    assert_rejected_without_writes(&fixture, "terrain.mclq_length");
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let fixture = Fixture::new();
        let mut adt = source_fixture("liquid_water_flat.adt.test-fixture");
        let cell = word(&adt, 92) as usize;
        let liquid = cell + word(&adt, cell + 8 + 0x60) as usize;
        replace(&mut adt, liquid + 20, value.to_bits());
        fixture.install_adt(adt);
        assert_rejected_without_writes(&fixture, "terrain.liquid_nonfinite");
    }
    let fixture = Fixture::new();
    let mut adt = source_fixture("liquid_water_flat.adt.test-fixture");
    for slot in 0..256 {
        let cell = word(&adt, 92 + slot * 16) as usize;
        let liquid = cell + word(&adt, cell + 8 + 0x60) as usize;
        adt[liquid + 664..liquid + 728].fill(0x0f);
    }
    fixture.install_adt(adt);
    assert_rejected_without_writes(&fixture, "terrain.liquid_empty_bounds");
}

#[test]
fn inspect_preserves_winner_mismatch_even_for_equal_member_bytes() {
    let fixture = Fixture::new();
    let bytes = fixture.install("flat");
    fs::write(fixture.data.join("patch-3.MPQ"), bytes).unwrap();
    let before = snapshot(&fixture.root);
    let output = fixture.run(&["--inspect"]);
    assert!(!output.status.success());
    let text = output_text(&output);
    assert!(text.contains("source.ad_winner_mismatch"), "{text}");
    assert!(!text.contains("AuthoredOracle"));
    assert_eq!(snapshot(&fixture.root), before);
}
#[test]
fn inspect_mode_conflict_cannot_create_requested_output() {
    let fixture = Fixture::new();
    fixture.install("flat");
    let before = snapshot(&fixture.root);
    let output = fixture.run(&["--inspect", "--output", "private-output"]);
    assert!(!output.status.success());
    assert!(output_text(&output).contains("cli.mode_conflict"));
    assert_eq!(snapshot(&fixture.root), before);
}
#[test]
fn ordinary_export_still_publishes_the_same_inspected_bytes() {
    let fixture = Fixture::new();
    fixture.install("flat");
    let inspection = fixture.run(&["--inspect"]);
    assert!(inspection.status.success());
    let output = fixture.run(&["--output", "export"]);
    assert!(output.status.success(), "{}", output_text(&output));
    let raw = fs::read(fixture.root.join("export/maps/0013132.map")).unwrap();
    assert_eq!(raw, include_bytes!("../tools/fixtures/flat.map"));
    let hash = format!("{:x}", Sha256::digest(&raw));
    assert!(output_text(&inspection).contains(&hash));
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("export/terrain.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["gameplay_admitted"], false);
    assert_eq!(manifest["live_install_lease"], false);
}

#[test]
fn liquid_export_publishes_exact_inspected_bytes_without_admission() {
    let fixture = Fixture::new();
    let source = fixture.install("liquid_nonsymmetric");
    let inspection = fixture.run(&["--inspect"]);
    assert!(inspection.status.success(), "{}", output_text(&inspection));
    let output = fixture.run(&["--output", "export"]);
    assert!(output.status.success(), "{}", output_text(&output));
    let raw = fs::read(fixture.root.join("export/maps/0013132.map")).unwrap();
    assert_eq!(raw, source_fixture("liquid_nonsymmetric.map"));
    assert!(output_text(&inspection).contains(&format!("{:x}", Sha256::digest(&raw))));
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("export/terrain.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["format"], "benilla-terrain-production-v3");
    assert_eq!(
        manifest["writer_policy"],
        "cmangos-ad-f0-boundary-history-closure-v3"
    );
    assert_eq!(manifest["gameplay_admitted"], false);
    assert_eq!(manifest["live_install_lease"], false);
    assert_eq!(manifest["products"].as_array().unwrap().len(), 1);
    assert!(!fixture.root.join("export/vmaps").exists());
    assert!(!fixture.root.join("export/mmaps").exists());
    assert_eq!(fs::read(fixture.data.join("terrain.MPQ")).unwrap(), source);
}

fn prefixed_header(bytes: &[u8], prefix: usize, header_size: u32) -> Vec<u8> {
    let mut result = vec![0; prefix];
    result.extend(bytes);
    result[prefix + 4..prefix + 8].copy_from_slice(&header_size.to_le_bytes());
    result
}

#[test]
fn inspect_accepts_source_compatible_headers_without_writes_or_product_changes() {
    let fixture = Fixture::new();
    let original = fixture.install("flat");
    let expected = fixture.run(&["--inspect"]);
    assert!(expected.status.success(), "{}", output_text(&expected));
    for (prefix, header_size) in [(0, 0), (0, u32::MAX), (512, 32), (1024, 1)] {
        fs::write(
            fixture.data.join("terrain.MPQ"),
            prefixed_header(&original, prefix, header_size),
        )
        .unwrap();
        let before = snapshot(&fixture.root);
        let output = fixture.run(&["--inspect"]);
        assert!(output.status.success(), "{}", output_text(&output));
        // The epoch binds complete consumed member bytes and winners, not unused prefix bytes.
        assert_eq!(output.stdout, expected.stdout);
        assert!(output.stderr.is_empty());
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn inspect_header_failures_are_precise_path_free_and_write_nothing() {
    let fixture = Fixture::new();
    let original = fixture.install("flat");
    let mut unsupported = prefixed_header(&original, 512, 32);
    unsupported[524..526].copy_from_slice(&2u16.to_le_bytes());
    // A valid later header must never rescue the first unsupported one.
    unsupported.resize(unsupported.len().next_multiple_of(512), 0);
    unsupported.extend(&original);

    let mut unaligned = prefixed_header(&original, 16, 32);
    unaligned[..4].copy_from_slice(b"MPQ\x1b");
    unaligned[8..12].copy_from_slice(&16u32.to_le_bytes());

    let mut mismatched = prefixed_header(&original, 512, 32);
    mismatched.resize(mismatched.len().next_multiple_of(512), 0);
    let second = mismatched.len() as u32;
    mismatched.extend(&original);
    mismatched[..4].copy_from_slice(b"MPQ\x1b");
    mismatched[8..12].copy_from_slice(&second.to_le_bytes());
    let source_size = (mismatched.len() - 512) as u32;
    mismatched[520..524].copy_from_slice(&source_size.to_le_bytes());

    let mut invalid_pointer = prefixed_header(&original, 512, 32);
    invalid_pointer[..4].copy_from_slice(b"MPQ\x1b");
    invalid_pointer[8..12].copy_from_slice(&16u32.to_le_bytes());

    let mut truncated = vec![0; 512];
    truncated.extend(b"MPQ\x1a");
    let mut wrong_size = prefixed_header(&original, 512, 32);
    let including_prefix = wrong_size.len() as u32;
    wrong_size[520..524].copy_from_slice(&including_prefix.to_le_bytes());
    for (bytes, id) in [
        (unsupported, "source.ad_header_version"),
        (unaligned, "source.ad_header_signature"),
        (mismatched, "source.ad_header_selection"),
        (invalid_pointer, "source.open_patch_chain"),
        (truncated, "source.ad_header_truncated"),
        (vec![0; 1024], "source.ad_header_signature"),
        (wrong_size, "source.ad_archive_size"),
        (
            prefixed_header(&original, 1024 * 1024, 32),
            "source.ad_header_scan_limit",
        ),
    ] {
        fs::write(fixture.data.join("terrain.MPQ"), bytes).unwrap();
        let before = snapshot(&fixture.root);
        let output = fixture.run(&["--inspect"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(text.starts_with(&format!("error[{id}]:")), "{text}");
        assert_eq!(text.lines().count(), 1);
        assert!(text.len() < 256);
        assert!(!text.contains('/'));
        assert!(!text.contains('\\'));
        assert!(!text.contains("AuthoredOracle"));
        assert!(!text.contains("private"));
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn inspect_version_one_normalizes_header_sizes_without_changing_products_or_epoch() {
    for case in ["flat", "liquid_nonsymmetric"] {
        let fixture = Fixture::new();
        let original = fixture.install(case);
        let product = source_fixture(&format!("{case}.map"));
        let epoch = assert_exact_inspection_without_writes(&fixture, &product);
        let bytes = version_one_archive(&terrain_entries(source_fixture(&format!(
            "{case}.adt.test-fixture"
        ))));
        assert_eq!(word(&bytes, 4), 44);
        assert_eq!(&bytes[12..16], &[1, 0, 3, 0]);
        assert_eq!(&bytes[32..44], &[0; 12]);
        assert_eq!(word(&bytes, 16), word(&original, 16) + 12);
        assert_eq!(word(&bytes, 20), word(&original, 20) + 12);
        assert_eq!(bytes.len(), original.len() + 12);
        for (prefix, header_size) in [
            (0, 44),
            (0, 0),
            (0, 32),
            (0, u32::MAX),
            (512, 44),
            (1024, 1),
        ] {
            fs::write(
                fixture.data.join("terrain.MPQ"),
                prefixed_header(&bytes, prefix, header_size),
            )
            .unwrap();
            assert_eq!(
                assert_exact_inspection_without_writes(&fixture, &product),
                epoch,
                "{case}: prefix={prefix} header_size={header_size}"
            );
        }
        let mut user_data = prefixed_header(&bytes, 512, 44);
        user_data[..4].copy_from_slice(b"MPQ\x1b");
        replace(&mut user_data, 8, 512);
        fs::write(fixture.data.join("terrain.MPQ"), user_data).unwrap();
        assert_eq!(
            assert_exact_inspection_without_writes(&fixture, &product),
            epoch
        );
    }
}

#[test]
fn inspect_mixed_legacy_and_version_one_archives_preserves_exact_member_epoch() {
    let fixture = Fixture::new();
    let entries = terrain_entries(source_fixture("liquid_nonsymmetric.adt.test-fixture"));
    let product = source_fixture("liquid_nonsymmetric.map");
    fs::write(fixture.data.join("dbc.MPQ"), archive(&entries[..3])).unwrap();
    fs::write(fixture.data.join("terrain.MPQ"), archive(&entries[3..])).unwrap();
    let epoch = assert_exact_inspection_without_writes(&fixture, &product);
    for (dbc_version_one, terrain_version_one) in [(true, false), (false, true), (true, true)] {
        for (name, members, version_one) in [
            ("dbc.MPQ", &entries[..3], dbc_version_one),
            ("terrain.MPQ", &entries[3..], terrain_version_one),
        ] {
            let bytes = if version_one {
                version_one_archive(members)
            } else {
                archive(members)
            };
            fs::write(fixture.data.join(name), bytes).unwrap();
        }
        assert_eq!(
            assert_exact_inspection_without_writes(&fixture, &product),
            epoch
        );
    }
}

#[test]
fn inspect_version_one_epoch_binds_unused_member_bytes_and_equal_byte_winners() {
    let fixture = Fixture::new();
    let mut entries = terrain_entries(source_fixture("flat.adt.test-fixture"));
    let path = fixture.data.join("terrain.MPQ");
    fs::write(&path, version_one_archive(&entries)).unwrap();
    let product = source_fixture("flat.map");
    let original_epoch = assert_exact_inspection_without_writes(&fixture, &product);
    let captured = prepare(&fixture.data, 1, &[Tile { x: 31, y: 32 }]).unwrap();

    let mut map = source_fixture("Map.dbc.test-fixture");
    // The writer uses Map.dbc fields zero and one, but captures every member byte.
    map[28] ^= 1;
    entries[0] = raw_entry("DBFilesClient\\Map.dbc", map);
    fs::write(&path, version_one_archive(&entries)).unwrap();
    let before = snapshot(&fixture.root);
    assert!(
        format!("{:#}", captured.verify_unchanged().unwrap_err()).contains("source.bytes_changed")
    );
    assert_eq!(snapshot(&fixture.root), before);
    let changed_epoch = assert_exact_inspection_without_writes(&fixture, &product);
    assert_ne!(changed_epoch, original_epoch);

    let captured = prepare(&fixture.data, 1, &[Tile { x: 31, y: 32 }]).unwrap();
    fs::write(
        fixture.data.join("patch.MPQ"),
        version_one_archive(&entries[..1]),
    )
    .unwrap();
    let before = snapshot(&fixture.root);
    assert!(
        format!("{:#}", captured.verify_unchanged().unwrap_err()).contains("source.winner_changed")
    );
    assert_eq!(snapshot(&fixture.root), before);
    assert_ne!(
        assert_exact_inspection_without_writes(&fixture, &product),
        changed_epoch
    );
}

#[test]
fn inspect_version_one_rejects_every_truncated_extension_without_writes() {
    let fixture = Fixture::new();
    let original = version_one_archive(&terrain_entries(source_fixture("flat.adt.test-fixture")));
    for prefix in [0, 512] {
        for extension_bytes in 0..12 {
            let mut bytes = prefixed_header(&original[..32 + extension_bytes], prefix, 44);
            replace(&mut bytes, prefix + 8, (32 + extension_bytes) as u32);
            fs::write(fixture.data.join("terrain.MPQ"), bytes).unwrap();
            assert_rejected_without_writes(&fixture, "source.ad_header_truncated");
        }
    }
}

#[test]
fn inspect_version_one_rejects_every_nonzero_extension_byte_without_writes() {
    let fixture = Fixture::new();
    let original = version_one_archive(&terrain_entries(source_fixture("flat.adt.test-fixture")));
    // Covers both halves of the u64 at 32 and both bytes of each u16 at 40 and 42.
    for offset in 32..44 {
        for value in [1, 0x80] {
            let mut bytes = prefixed_header(&original, 512, 44);
            bytes[512 + offset] = value;
            fs::write(fixture.data.join("terrain.MPQ"), bytes).unwrap();
            assert_rejected_without_writes(&fixture, "source.ad_header_extension");
        }
    }
}

#[test]
fn inspect_version_one_rejects_indices_inside_its_extension_without_writes() {
    let fixture = Fixture::new();
    let original = version_one_archive(&terrain_entries(source_fixture("flat.adt.test-fixture")));
    for field in [16, 20] {
        for position in 32..44 {
            let mut bytes = original.clone();
            replace(&mut bytes, field, position);
            fs::write(fixture.data.join("terrain.MPQ"), bytes).unwrap();
            assert_rejected_without_writes(&fixture, "source.ad_index_spans");
        }
    }
}

#[test]
fn inspect_rejects_versions_above_one_without_writes() {
    let fixture = Fixture::new();
    let original = version_one_archive(&terrain_entries(source_fixture("flat.adt.test-fixture")));
    for version in [2u16, 3, 4, 0x100, u16::MAX] {
        let mut bytes = original.clone();
        bytes[12..14].copy_from_slice(&version.to_le_bytes());
        fs::write(fixture.data.join("terrain.MPQ"), bytes).unwrap();
        assert_rejected_without_writes(&fixture, "source.ad_header_version");
    }
}

#[test]
fn inspect_never_rescues_a_bad_first_header_with_a_later_version_one_archive() {
    let fixture = Fixture::new();
    let original = version_one_archive(&terrain_entries(source_fixture("flat.adt.test-fixture")));
    for (field, value, expected) in [
        (12, 2, "source.ad_header_version"),
        (36, 1, "source.ad_header_extension"),
        (40, 1, "source.ad_header_extension"),
        (42, 1, "source.ad_header_extension"),
        (16, 32, "source.ad_index_spans"),
    ] {
        let mut bytes = prefixed_header(&original, 512, 44);
        bytes[512 + field] = value;
        bytes.resize(bytes.len().next_multiple_of(512), 0);
        bytes.extend(&original);
        let archive_bytes = (bytes.len() - 512) as u32;
        replace(&mut bytes, 520, archive_bytes);
        fs::write(fixture.data.join("terrain.MPQ"), bytes).unwrap();
        assert_rejected_without_writes(&fixture, expected);
    }
}

#[test]
fn inspect_version_one_rejects_user_data_pointer_header_mismatch_without_writes() {
    let fixture = Fixture::new();
    let entries = terrain_entries(source_fixture("flat.adt.test-fixture"));
    let original = version_one_archive(&entries);
    for second in [archive(&entries), original.clone()] {
        let mut bytes = prefixed_header(&original, 512, 44);
        bytes.resize(bytes.len().next_multiple_of(512), 0);
        let second_offset = bytes.len() as u32;
        bytes.extend(second);
        bytes[..4].copy_from_slice(b"MPQ\x1b");
        replace(&mut bytes, 8, second_offset);
        let archive_bytes = (bytes.len() - 512) as u32;
        replace(&mut bytes, 520, archive_bytes);
        fs::write(fixture.data.join("terrain.MPQ"), bytes).unwrap();
        assert_rejected_without_writes(&fixture, "source.ad_header_selection");
    }
}

#[test]
fn diagnose_dry_reports_actual_raw_adt_and_selected_map_row_without_writes() {
    let fixture = Fixture::new();
    let map = map_dbc(&[
        (1, b"Kalimdor"),
        (900, b""),
        (0, b"Azeroth"),
        (901, b"Prototype Zone"),
        (902, b"../Unused"),
        (903, b"D\\Unused"),
    ]);
    fs::write(
        fixture.data.join("terrain.MPQ"),
        archive(&selected_azeroth_entries(map)),
    )
    .unwrap();
    let selected = Tile { x: 48, y: 32 };
    let report = diagnostic_report(&fixture, 0, selected, &["--diagnose"]);
    assert_eq!(
        report["map_table"],
        json!({"selected_row_ordinal": 2, "total_rows": 6})
    );
    let prepared = prepare(&fixture.data, 0, &[selected]).unwrap();
    assert_eq!(report["source_epoch"], prepared.source_epoch());
    for stage in DIAGNOSTIC_STAGES {
        assert_stage(&report["stages"], stage, "passed", None);
    }
    assert_eq!(report["tiles"].as_array().unwrap().len(), 1);
    let tile = &report["tiles"][0];
    assert_eq!(tile["x"], 48);
    assert_eq!(tile["y"], 32);
    assert_raw_adt(tile, &source_fixture("flat.adt.test-fixture"));
    assert_eq!(tile["active_mclq_cells"], 0);
    assert!(tile["liquid_rectangle"].is_null());
    assert!(tile["missing_liquid_heights"].is_null());
    for stage in TILE_DIAGNOSTIC_STAGES {
        assert_stage(
            &tile["stages"],
            stage,
            if stage == "history_resolution" {
                "unavailable"
            } else {
                "passed"
            },
            None,
        );
    }
    assert!(tile["history"].is_null());
    assert_eq!(
        report,
        diagnostic_report(&fixture, 0, selected, &["--diagnose"])
    );
}

#[test]
fn diagnose_liquid_history_preserves_raw_summary_and_reports_initial_zero_closure() {
    let fixture = Fixture::new();
    let mut previous = None;
    for case in ["liquid_stale_edge_a", "liquid_stale_edge_b"] {
        let adt = source_fixture(&format!("{case}.adt.test-fixture"));
        fixture.install_adt(adt.clone());
        let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
        assert_sha256(&report["source_epoch"]);
        for stage in DIAGNOSTIC_STAGES {
            assert_stage(&report["stages"], stage, "passed", None);
        }
        let tile = &report["tiles"][0];
        assert_raw_adt(tile, &adt);
        assert_eq!(tile["active_mclq_cells"], 2);
        assert_eq!(
            tile["liquid_rectangle"],
            json!({"x": 29, "y": 19, "width": 100, "height": 110})
        );
        // The authored oracle has 199 predecessor-dependent samples; row-major first
        // is the right edge immediately after the selected cell's owned y=24 seam.
        assert_eq!(
            tile["missing_liquid_heights"],
            json!({"count": 199, "first_x": 128, "first_y": 25})
        );
        assert_stage(&tile["stages"], "wdt_selection", "passed", None);
        assert_stage(&tile["stages"], "adt_structure", "passed", None);
        assert_stage(
            &tile["stages"],
            "liquid",
            "failed",
            Some("terrain.liquid_history"),
        );
        assert_stage(&tile["stages"], "history_resolution", "passed", None);
        assert_stage(&tile["stages"], "production", "passed", None);
        assert_eq!(tile["history"]["initial_zero_samples"], 199);
        assert_eq!(tile["history"]["consulted_adts"], 0);
        if let Some(previous) = &previous {
            assert_eq!(&report, previous);
        }
        previous = Some(report.clone());
        let before = snapshot(&fixture.root);
        let prepared = prepare(&fixture.data, 1, &[Tile { x: 31, y: 32 }]).unwrap();
        assert_eq!(report["source_epoch"], prepared.source_epoch());
        let output = fixture.run(&["--inspect"]);
        assert!(output.status.success(), "{}", output_text(&output));
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn diagnose_liquid_counts_only_heights_the_actual_payload_would_serialize() {
    let fixture = Fixture::new();
    for (case, cells, rectangle) in [
        ("liquid_water_flat", 256, [0, 0, 129, 129]),
        ("liquid_nonsymmetric", 1, [57, 26, 6, 4]),
        ("liquid_outer_edges", 1, [121, 122, 8, 7]),
        ("liquid_sparse_flat", 2, [29, 19, 100, 110]),
    ] {
        let adt = source_fixture(&format!("{case}.adt.test-fixture"));
        fixture.install_adt(adt.clone());
        let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
        let tile = &report["tiles"][0];
        assert_raw_adt(tile, &adt);
        assert_eq!(tile["active_mclq_cells"], cells, "{case}");
        assert_eq!(
            tile["liquid_rectangle"],
            json!({
                "x": rectangle[0], "y": rectangle[1], "width": rectangle[2], "height": rectangle[3],
            }),
            "{case}"
        );
        assert_no_missing_heights(tile);
        for stage in TILE_DIAGNOSTIC_STAGES {
            assert_stage(
                &tile["stages"],
                stage,
                if stage == "history_resolution" {
                    "unavailable"
                } else {
                    "passed"
                },
                None,
            );
        }
        assert!(tile["history"].is_null());
    }
    for (case, cells) in [
        ("liquid_empty", 0),
        ("liquid_absent_flags", 0),
        ("liquid_untyped", 256),
    ] {
        fixture.install(case);
        let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
        let tile = &report["tiles"][0];
        assert_eq!(tile["active_mclq_cells"], cells, "{case}");
        assert!(tile["liquid_rectangle"].is_null(), "{case}");
        assert!(tile["missing_liquid_heights"].is_null(), "{case}");
        assert_stage(&tile["stages"], "production", "passed", None);
    }
}

#[test]
fn diagnose_keeps_table_and_adt_checks_independent() {
    let fixture = Fixture::new();
    for (index, name, stage, id) in [
        (
            1,
            "DBFilesClient\\AreaTable.dbc",
            "area_table",
            "terrain.AreaTable",
        ),
        (
            2,
            "DBFilesClient\\LiquidType.dbc",
            "liquid_table",
            "terrain.LiquidType",
        ),
    ] {
        let adt = source_fixture("liquid_nonsymmetric.adt.test-fixture");
        let mut entries = terrain_entries(adt.clone());
        entries[index] = raw_entry(name, b"SECRET invalid /private/player table bytes".to_vec());
        fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
        let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
        assert_sha256(&report["source_epoch"]);
        for other in DIAGNOSTIC_STAGES {
            assert_stage(
                &report["stages"],
                other,
                if other == stage { "failed" } else { "passed" },
                if other == stage { Some(id) } else { None },
            );
        }
        let tile = &report["tiles"][0];
        assert_raw_adt(tile, &adt);
        for other in ["wdt_selection", "adt_structure", "liquid"] {
            assert_stage(&tile["stages"], other, "passed", None);
        }
        assert_stage(&tile["stages"], "production", "unavailable", None);
        assert_no_missing_heights(tile);
    }
    let mut adt = source_fixture("flat.adt.test-fixture");
    replace(&mut adt, 8, 17);
    fixture.install_adt(adt.clone());
    let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
    assert_sha256(&report["source_epoch"]);
    for stage in DIAGNOSTIC_STAGES {
        assert_stage(&report["stages"], stage, "passed", None);
    }
    let tile = &report["tiles"][0];
    assert_raw_adt(tile, &adt);
    assert_stage(&tile["stages"], "wdt_selection", "passed", None);
    assert_stage(
        &tile["stages"],
        "adt_structure",
        "failed",
        Some("terrain.adt_version"),
    );
    for stage in ["liquid", "history_resolution", "production"] {
        assert_stage(&tile["stages"], stage, "unavailable", None);
    }
    for field in [
        "active_mclq_cells",
        "liquid_rectangle",
        "missing_liquid_heights",
        "history",
    ] {
        assert!(tile[field].is_null());
    }
}

#[test]
fn diagnose_absent_or_invalid_source_has_no_epoch_or_invented_results() {
    let fixture = Fixture::new();
    fs::remove_dir(&fixture.data).unwrap();
    for (present, id) in [
        (false, "source.data_directory_required"),
        (true, "source.ad_header_signature"),
    ] {
        if present {
            fs::create_dir(&fixture.data).unwrap();
            fs::write(
                fixture.data.join("terrain.MPQ"),
                b"SECRET /private/player invalid archive",
            )
            .unwrap();
        }
        let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
        assert!(report["source_epoch"].is_null());
        assert!(report["map_table"].is_null());
        assert_stage(&report["stages"], "selection", "passed", None);
        assert_stage(&report["stages"], "initial_capture", "failed", Some(id));
        for stage in &DIAGNOSTIC_STAGES[2..] {
            assert_stage(&report["stages"], stage, "unavailable", None);
        }
        assert_eq!(report["tiles"].as_array().unwrap().len(), 1);
        let tile = &report["tiles"][0];
        for field in [
            "raw_adt",
            "active_mclq_cells",
            "liquid_rectangle",
            "missing_liquid_heights",
            "history",
        ] {
            assert!(tile[field].is_null());
        }
        for stage in TILE_DIAGNOSTIC_STAGES {
            assert_stage(&tile["stages"], stage, "unavailable", None);
        }
    }
}

#[test]
fn diagnose_semantic_selection_failures_are_json_before_any_source_work() {
    let fixture = Fixture::new();
    fixture.install("flat");
    for (map, tile, extra, id) in [
        (1000, Tile { x: 31, y: 32 }, vec![], "selection.bounds"),
        (u32::MAX, Tile { x: 31, y: 32 }, vec![], "selection.bounds"),
        (1, Tile { x: 64, y: 32 }, vec![], "selection.tile_bounds"),
        (1, Tile { x: 31, y: 255 }, vec![], "selection.tile_bounds"),
        (
            1,
            Tile { x: 31, y: 32 },
            vec!["--tile", "31,32"],
            "selection.duplicate_tile",
        ),
        (
            1,
            Tile { x: 31, y: 32 },
            vec![
                "--tile", "0,0", "--tile", "0,1", "--tile", "0,2", "--tile", "0,3", "--tile",
                "0,4", "--tile", "0,5", "--tile", "0,6", "--tile", "0,7", "--tile", "0,8",
            ],
            "selection.bounds",
        ),
    ] {
        let mut args = vec!["--diagnose"];
        args.extend(extra);
        let report = diagnostic_report(&fixture, map, tile, &args);
        assert_stage(&report["stages"], "selection", "failed", Some(id));
        for stage in &DIAGNOSTIC_STAGES[1..] {
            assert_stage(&report["stages"], stage, "unavailable", None);
        }
        assert!(report["source_epoch"].is_null());
        assert!(report["map_table"].is_null());
        assert!(report["tiles"].as_array().unwrap().is_empty());
    }
}

#[test]
fn diagnose_malformed_selected_tile_does_not_mask_another_tiles_production() {
    let fixture = Fixture::new();
    let good = source_fixture("flat.adt.test-fixture");
    let mut bad = good.clone();
    replace(&mut bad, 8, 17);
    let mut entries = terrain_entries(good.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let mut wdt = source_fixture("source.wdt.test-fixture");
    replace(&mut wdt, 60 + (31 * 64 + 33) * 8, 1);
    entries[3] = raw_entry("World\\Maps\\AuthoredOracle\\AuthoredOracle.wdt", wdt);
    entries.push(raw_entry(
        "World\\Maps\\AuthoredOracle\\AuthoredOracle_33_31.adt",
        bad.clone(),
    ));
    fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
    // Submit reverse order to check deterministic numeric tile ordering too.
    let report = diagnostic_report(
        &fixture,
        1,
        Tile { x: 31, y: 33 },
        &["--diagnose", "--tile", "31,32"],
    );
    assert_sha256(&report["source_epoch"]);
    for stage in DIAGNOSTIC_STAGES {
        assert_stage(&report["stages"], stage, "passed", None);
    }
    let tiles = report["tiles"].as_array().unwrap();
    assert_eq!(tiles.len(), 2);
    assert_eq!(
        (tiles[0]["x"].as_u64(), tiles[0]["y"].as_u64()),
        (Some(31), Some(32))
    );
    assert_raw_adt(&tiles[0], &good);
    for stage in TILE_DIAGNOSTIC_STAGES {
        assert_stage(
            &tiles[0]["stages"],
            stage,
            if stage == "history_resolution" {
                "unavailable"
            } else {
                "passed"
            },
            None,
        );
    }
    assert_eq!(
        (tiles[1]["x"].as_u64(), tiles[1]["y"].as_u64()),
        (Some(31), Some(33))
    );
    assert_raw_adt(&tiles[1], &bad);
    assert_stage(&tiles[1]["stages"], "wdt_selection", "passed", None);
    assert_stage(
        &tiles[1]["stages"],
        "adt_structure",
        "failed",
        Some("terrain.adt_version"),
    );
    for stage in ["liquid", "history_resolution", "production"] {
        assert_stage(&tiles[1]["stages"], stage, "unavailable", None);
    }
}

#[test]
fn diagnose_bad_map_metadata_or_wdt_selection_never_fabricates_a_full_epoch() {
    let fixture = Fixture::new();
    let mut entries = terrain_entries(source_fixture("flat.adt.test-fixture"));
    entries[0] = raw_entry("DBFilesClient\\Map.dbc", map_dbc(&[(1, b"SECRET/private")]));
    fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
    let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
    assert!(report["source_epoch"].is_null());
    assert!(report["map_table"].is_null());
    assert_stage(
        &report["stages"],
        "map_table",
        "failed",
        Some("terrain.map_directory"),
    );
    for stage in ["selection", "initial_capture", "area_table", "liquid_table"] {
        assert_stage(&report["stages"], stage, "passed", None);
    }
    for stage in ["selection_capture", "full_capture", "source_recheck"] {
        assert_stage(&report["stages"], stage, "unavailable", None);
    }
    fixture.install("flat");
    let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 33 }, &["--diagnose"]);
    assert!(report["source_epoch"].is_null());
    assert_eq!(
        report["map_table"],
        json!({"selected_row_ordinal": 0, "total_rows": 1})
    );
    assert_stage(&report["stages"], "selection_capture", "passed", None);
    assert_stage(&report["stages"], "full_capture", "unavailable", None);
    assert_stage(&report["stages"], "source_recheck", "unavailable", None);
    let tile = &report["tiles"][0];
    assert!(tile["raw_adt"].is_null());
    assert_stage(
        &tile["stages"],
        "wdt_selection",
        "failed",
        Some("terrain.wdt_tile_absent"),
    );
    for stage in [
        "adt_structure",
        "liquid",
        "history_resolution",
        "production",
    ] {
        assert_stage(&tile["stages"], stage, "unavailable", None);
    }
}

fn wdt_member(directory: &str, tiles: &[Tile]) -> Entry {
    raw_entry(
        &format!("World\\Maps\\{directory}\\{directory}.wdt"),
        wdt_for_tiles(tiles),
    )
}

fn wdt_for_tiles(tiles: &[Tile]) -> Vec<u8> {
    let mut bytes = source_fixture("source.wdt.test-fixture");
    assert_eq!(&bytes[52..56], b"NIAM");
    bytes[60..60 + 64 * 64 * 8].fill(0);
    for tile in tiles {
        replace(
            &mut bytes,
            60 + (usize::from(tile.x) * 64 + usize::from(tile.y)) * 8,
            1,
        );
    }
    bytes
}

fn adt_member(directory: &str, tile: Tile, bytes: Vec<u8>) -> Entry {
    raw_entry(
        &format!(
            "World\\Maps\\{directory}\\{directory}_{}_{}.adt",
            tile.y, tile.x
        ),
        bytes,
    )
}

fn history_entries(case: &str, predecessors: &[&str]) -> Vec<Entry> {
    let selected = Tile { x: 31, y: 32 };
    let mut entries = terrain_entries(source_fixture(&format!("{case}.adt.test-fixture")))
        .into_iter()
        .collect::<Vec<_>>();
    assert!(predecessors.len() <= 32);
    let mut tiles = vec![selected];
    for (index, predecessor) in predecessors.iter().enumerate() {
        let tile = Tile {
            x: 31,
            y: 32 - predecessors.len() as u8 + index as u8,
        };
        tiles.push(tile);
        entries.push(adt_member(
            "AuthoredOracle",
            tile,
            source_fixture(&format!("{predecessor}.adt.test-fixture")),
        ));
    }
    entries[3] = wdt_member("AuthoredOracle", &tiles);
    entries
}

fn install_history(fixture: &Fixture, case: &str, predecessors: &[&str]) {
    fs::write(
        fixture.data.join("terrain.MPQ"),
        archive(&history_entries(case, predecessors)),
    )
    .unwrap();
}

#[test]
fn inspect_exact61_matches_independent_source_history_vectors_without_writes() {
    for (suffix, predecessors, consulted) in [
        ("initial", vec![], 0),
        ("positive", vec!["liquid_water_flat"], 1),
        (
            "negative",
            vec!["liquid_water_flat", "liquid_ocean_flat"],
            1,
        ),
        ("dry", vec!["liquid_ocean_flat", "flat"], 2),
        ("untyped", vec!["liquid_ocean_flat", "liquid_untyped"], 1),
        (
            "hidden",
            vec!["liquid_water_flat", "liquid_history_hidden_seed"],
            1,
        ),
        ("seams", vec!["liquid_seams"], 1),
        (
            "partial",
            vec!["liquid_ocean_flat", "liquid_history_partial_seed"],
            2,
        ),
    ] {
        let fixture = Fixture::new();
        let case = format!("liquid_history61_{suffix}");
        install_history(&fixture, &case, &predecessors);
        let epoch = assert_exact_inspection_without_writes(
            &fixture,
            &source_fixture(&format!("{case}.map")),
        );
        let report = diagnostic_report(&fixture, 1, Tile { x: 31, y: 32 }, &["--diagnose"]);
        assert_eq!(report["source_epoch"], epoch, "{suffix}");
        let tile = &report["tiles"][0];
        assert_eq!(
            tile["liquid_rectangle"],
            json!({"x": 47, "y": 0, "width": 79, "height": 129}),
            "{suffix}"
        );
        assert_eq!(
            tile["missing_liquid_heights"],
            json!({"count": 61, "first_x": 65, "first_y": 128}),
            "{suffix}"
        );
        assert_stage(
            &tile["stages"],
            "liquid",
            "failed",
            Some("terrain.liquid_history"),
        );
        assert_stage(&tile["stages"], "history_resolution", "passed", None);
        assert_stage(&tile["stages"], "production", "passed", None);
        assert_eq!(
            tile["history"],
            json!({
                "consulted_adts": consulted, "consulted_wdts": 1,
                "absent_members": 0, "initial_zero_samples": if suffix == "initial" { 61 } else { 0 },
            }),
            "{suffix}"
        );
    }
}

#[test]
fn history_uses_raw_map_row_order_and_never_reads_future_or_exhausted_predecessors() {
    let fixture = Fixture::new();
    let selected = Tile { x: 31, y: 32 };
    let future = Tile { x: 31, y: 33 };
    let prior = Tile { x: 63, y: 63 };
    let mut entries = terrain_entries(source_fixture("liquid_history61_negative.adt.test-fixture"))
        .into_iter()
        .collect::<Vec<_>>();
    // Raw record order is deliberately neither numeric-ID nor directory-name order.
    entries[0] = raw_entry(
        "DBFilesClient\\Map.dbc",
        map_dbc(&[
            (900, b"OldHistory"),
            (700, b"PriorHistory"),
            (1, b"AuthoredOracle"),
            (0, b"FutureHistory"),
        ]),
    );
    entries[3] = wdt_member("AuthoredOracle", &[selected, future]);
    entries.extend([
        wdt_member("PriorHistory", &[prior]),
        adt_member(
            "PriorHistory",
            prior,
            source_fixture("liquid_ocean_flat.adt.test-fixture"),
        ),
        // Neither a future event nor an older-than-needed map may be parsed or bound.
        adt_member(
            "AuthoredOracle",
            future,
            b"SECRET malformed future ADT".to_vec(),
        ),
        raw_entry(
            "World\\Maps\\OldHistory\\OldHistory.wdt",
            b"SECRET malformed old WDT".to_vec(),
        ),
        raw_entry(
            "World\\Maps\\FutureHistory\\FutureHistory.wdt",
            b"SECRET malformed future WDT".to_vec(),
        ),
    ]);
    let path = fixture.data.join("terrain.MPQ");
    fs::write(&path, archive(&entries)).unwrap();
    let expected = source_fixture("liquid_history61_negative.map");
    let epoch = assert_exact_inspection_without_writes(&fixture, &expected);
    let captured = prepare(&fixture.data, 1, &[selected]).unwrap();
    entries[7] = adt_member(
        "AuthoredOracle",
        future,
        source_fixture("liquid_water_flat.adt.test-fixture"),
    );
    entries[8] = wdt_member("OldHistory", &[Tile { x: 0, y: 0 }]);
    entries[9] = wdt_member("FutureHistory", &[Tile { x: 0, y: 0 }]);
    fs::write(&path, archive(&entries)).unwrap();
    captured.verify_unchanged().unwrap();
    assert_eq!(
        assert_exact_inspection_without_writes(&fixture, &expected),
        epoch
    );
    let report = diagnostic_report(&fixture, 1, selected, &["--diagnose"]);
    assert_eq!(
        report["map_table"],
        json!({"selected_row_ordinal": 2, "total_rows": 4})
    );
    assert_eq!(
        report["tiles"][0]["history"],
        json!({
            "consulted_adts": 1, "consulted_wdts": 2, "absent_members": 0, "initial_zero_samples": 0,
        })
    );
}

#[test]
fn multiple_selected_tiles_resolve_independent_histories_in_source_order() {
    let fixture = Fixture::new();
    let first = Tile { x: 31, y: 32 };
    let second = Tile { x: 31, y: 34 };
    let mut entries = history_entries("liquid_stale_edge_a", &["liquid_water_flat"]);
    entries[3] = wdt_member(
        "AuthoredOracle",
        &[Tile { x: 31, y: 31 }, first, Tile { x: 31, y: 33 }, second],
    );
    entries.push(adt_member(
        "AuthoredOracle",
        Tile { x: 31, y: 33 },
        source_fixture("liquid_ocean_flat.adt.test-fixture"),
    ));
    entries.push(adt_member(
        "AuthoredOracle",
        second,
        source_fixture("liquid_stale_edge_b.adt.test-fixture"),
    ));
    fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
    let before = snapshot(&fixture.root);
    let first_expected = source_fixture("liquid_stale_edge_a.map");
    let second_expected = source_fixture("liquid_stale_edge_b.map");
    let prepared = prepare(&fixture.data, 1, &[second, first]).unwrap();
    assert_eq!(
        prepared.tiles().collect::<Vec<_>>(),
        [
            (first, first_expected.as_slice()),
            (second, second_expected.as_slice()),
        ]
    );
    prepared.verify_unchanged().unwrap();
    let reverse = fixture.run_selection(1, second, &["--inspect", "--tile", "31,32"]);
    let forward = fixture.run_selection(1, first, &["--inspect", "--tile", "31,34"]);
    assert!(reverse.status.success(), "{}", output_text(&reverse));
    assert!(forward.status.success(), "{}", output_text(&forward));
    assert_eq!(reverse.stdout, forward.stdout);
    assert!(reverse.stderr.is_empty());
    let text = output_text(&reverse);
    for expected in [&first_expected, &second_expected] {
        assert!(text.contains(&format!("{:x}", Sha256::digest(expected))));
    }
    assert_eq!(snapshot(&fixture.root), before);
    let report = diagnostic_report(&fixture, 1, second, &["--diagnose", "--tile", "31,32"]);
    assert_eq!(report["source_epoch"], prepared.source_epoch());
    for tile in report["tiles"].as_array().unwrap() {
        assert_eq!(tile["history"]["consulted_adts"], 1);
        assert_stage(&tile["stages"], "history_resolution", "passed", None);
        assert_stage(&tile["stages"], "production", "passed", None);
    }
}

fn cross_map_history_entries() -> Vec<Entry> {
    let mut entries = terrain_entries(source_fixture("liquid_history61_positive.adt.test-fixture"))
        .into_iter()
        .collect::<Vec<_>>();
    entries[0] = raw_entry(
        "DBFilesClient\\Map.dbc",
        map_dbc(&[(700, b"PriorHistory"), (1, b"AuthoredOracle")]),
    );
    entries.extend([
        wdt_member("PriorHistory", &[Tile { x: 0, y: 0 }]),
        adt_member(
            "PriorHistory",
            Tile { x: 0, y: 0 },
            source_fixture("liquid_water_flat.adt.test-fixture"),
        ),
    ]);
    entries
}

#[test]
fn prepared_history_binds_selected_and_predecessor_bytes_and_equal_byte_winners() {
    for (index, name, bytes) in [
        (
            3,
            "World\\Maps\\AuthoredOracle\\AuthoredOracle.wdt",
            source_fixture("source.wdt.test-fixture"),
        ),
        (
            4,
            "World\\Maps\\AuthoredOracle\\AuthoredOracle_32_31.adt",
            source_fixture("liquid_history61_positive.adt.test-fixture"),
        ),
        (
            5,
            "World\\Maps\\PriorHistory\\PriorHistory.wdt",
            wdt_for_tiles(&[Tile { x: 0, y: 0 }]),
        ),
        (
            6,
            "World\\Maps\\PriorHistory\\PriorHistory_0_0.adt",
            source_fixture("liquid_water_flat.adt.test-fixture"),
        ),
    ] {
        let fixture = Fixture::new();
        let path = fixture.data.join("terrain.MPQ");
        let mut entries = cross_map_history_entries();
        fs::write(&path, archive(&entries)).unwrap();
        let captured = prepare(&fixture.data, 1, &[Tile { x: 31, y: 32 }]).unwrap();
        let original_epoch = captured.source_epoch().to_owned();
        let mut altered = bytes.clone();
        // Every consumed byte is evidence even if that field is unused by the product.
        let last = altered.len() - 1;
        altered[last] ^= 1;
        entries[index] = raw_entry(name, altered);
        fs::write(&path, archive(&entries)).unwrap();
        let before = snapshot(&fixture.root);
        assert!(
            format!("{:#}", captured.verify_unchanged().unwrap_err())
                .contains("source.bytes_changed"),
            "{name}"
        );
        assert!(
            format!(
                "{:#}",
                captured
                    .publish(&fixture.root.join("stale-export"))
                    .unwrap_err()
            )
            .contains("source.bytes_changed"),
            "{name}"
        );
        assert_eq!(snapshot(&fixture.root), before);

        entries[index] = raw_entry(name, bytes.clone());
        fs::write(&path, archive(&entries)).unwrap();
        let captured = prepare(&fixture.data, 1, &[Tile { x: 31, y: 32 }]).unwrap();
        assert_eq!(captured.source_epoch(), original_epoch);
        fs::write(
            fixture.data.join("patch.MPQ"),
            archive(&[raw_entry(name, bytes)]),
        )
        .unwrap();
        let before = snapshot(&fixture.root);
        assert!(
            format!("{:#}", captured.verify_unchanged().unwrap_err())
                .contains("source.winner_changed"),
            "{name}"
        );
        assert!(
            format!(
                "{:#}",
                captured
                    .publish(&fixture.root.join("winner-export"))
                    .unwrap_err()
            )
            .contains("source.winner_changed"),
            "{name}"
        );
        assert_eq!(snapshot(&fixture.root), before);
        assert_ne!(
            assert_exact_inspection_without_writes(
                &fixture,
                &source_fixture("liquid_history61_positive.map")
            ),
            original_epoch
        );
    }
}

#[test]
fn absent_predecessor_wdt_and_adt_receipts_reject_later_insertions() {
    for absent_adt in [false, true] {
        let fixture = Fixture::new();
        let mut entries =
            terrain_entries(source_fixture("liquid_history61_initial.adt.test-fixture"))
                .into_iter()
                .collect::<Vec<_>>();
        entries[0] = raw_entry(
            "DBFilesClient\\Map.dbc",
            map_dbc(&[(700, b"PriorHistory"), (1, b"AuthoredOracle")]),
        );
        if absent_adt {
            entries.push(wdt_member("PriorHistory", &[Tile { x: 0, y: 0 }]));
        }
        fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
        let expected = source_fixture("liquid_history61_initial.map");
        let epoch = assert_exact_inspection_without_writes(&fixture, &expected);
        let selected = Tile { x: 31, y: 32 };
        prepare(&fixture.data, 1, &[selected])
            .unwrap()
            .publish(&fixture.root.join("receipt-export"))
            .unwrap();
        let manifest: Value = serde_json::from_slice(
            &fs::read(fixture.root.join("receipt-export/terrain.json")).unwrap(),
        )
        .unwrap();
        let absent_name = if absent_adt {
            "World\\Maps\\PriorHistory\\PriorHistory_0_0.adt"
        } else {
            "World\\Maps\\PriorHistory\\PriorHistory.wdt"
        };
        assert_eq!(manifest["absent_members"], json!([{"name": absent_name}]));
        let captured = prepare(&fixture.data, 1, &[selected]).unwrap();
        entries.push(if absent_adt {
            adt_member(
                "PriorHistory",
                Tile { x: 0, y: 0 },
                source_fixture("flat.adt.test-fixture"),
            )
        } else {
            wdt_member("PriorHistory", &[])
        });
        fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
        let before = snapshot(&fixture.root);
        assert!(format!("{:#}", captured.verify_unchanged().unwrap_err())
            .contains("source.absence_changed"));
        assert!(format!(
            "{:#}",
            captured
                .publish(&fixture.root.join("stale-export"))
                .unwrap_err()
        )
        .contains("source.absence_changed"));
        assert_eq!(snapshot(&fixture.root), before);
        // These insertions produce no boundary writes, but alter the closure evidence.
        assert_ne!(
            assert_exact_inspection_without_writes(&fixture, &expected),
            epoch
        );
    }
}

#[test]
fn malformed_or_mh2o_predecessor_cannot_be_silently_treated_as_absent_or_zero() {
    for (mutation, expected) in [
        (0, "terrain.adt_version"),
        (1, "terrain.mh2o_excluded"),
        (2, "terrain.mclq_length"),
        (3, "terrain.liquid_nonfinite"),
    ] {
        let fixture = Fixture::new();
        let mut entries = history_entries("liquid_history61_positive", &["liquid_water_flat"]);
        let mut predecessor = source_fixture("liquid_water_flat.adt.test-fixture");
        let cell = word(&predecessor, 92) as usize;
        let liquid = cell + word(&predecessor, cell + 8 + 0x60) as usize;
        match mutation {
            0 => replace(&mut predecessor, 8, 17),
            1 => replace(&mut predecessor, 20 + 0x28, 1),
            2 => replace(&mut predecessor, liquid + 4, 803),
            3 => replace(&mut predecessor, liquid + 20, f32::NAN.to_bits()),
            _ => unreachable!(),
        }
        entries[5] = adt_member("AuthoredOracle", Tile { x: 31, y: 31 }, predecessor);
        fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
        assert_rejected_without_writes(&fixture, expected);
    }
}

#[test]
fn resolved_history_export_publishes_exact_bytes_and_receipts_without_replacing_output() {
    let fixture = Fixture::new();
    install_history(
        &fixture,
        "liquid_history61_partial",
        &["liquid_ocean_flat", "liquid_history_partial_seed"],
    );
    let expected = source_fixture("liquid_history61_partial.map");
    let epoch = assert_exact_inspection_without_writes(&fixture, &expected);
    let output = fixture.run(&["--output", "export"]);
    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read(fixture.root.join("export/maps/0013132.map")).unwrap(),
        expected
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("export/terrain.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["format"], "benilla-terrain-production-v3");
    assert_eq!(
        manifest["writer_policy"],
        "cmangos-ad-f0-boundary-history-closure-v3"
    );
    assert_eq!(manifest["source_epoch"], epoch);
    assert_eq!(manifest["absent_members"], json!([]));
    assert_eq!(manifest["members"].as_array().unwrap().len(), 7);
    assert_eq!(manifest["products"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["gameplay_admitted"], false);
    assert_eq!(manifest["live_install_lease"], false);
    for name in ["AuthoredOracle_30_31.adt", "AuthoredOracle_31_31.adt"] {
        assert!(manifest["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["name"].as_str().unwrap().ends_with(name)));
    }
    let before = snapshot(&fixture.root);
    let output = fixture.run(&["--output", "export"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output_text(&output).contains("output.already_exists"));
    assert_eq!(snapshot(&fixture.root), before);
    assert!(!fixture.root.join("export/terrain.pending").exists());
}

#[test]
fn history_traverses_wdt_rows_before_columns_and_ignores_a_future_row() {
    let fixture = Fixture::new();
    let selected = Tile { x: 31, y: 32 };
    let prior = Tile { x: 30, y: 63 };
    let future = Tile { x: 32, y: 0 };
    let mut entries = terrain_entries(source_fixture("liquid_history61_positive.adt.test-fixture"))
        .into_iter()
        .collect::<Vec<_>>();
    entries[3] = wdt_member("AuthoredOracle", &[prior, selected, future]);
    entries.extend([
        adt_member(
            "AuthoredOracle",
            prior,
            source_fixture("liquid_water_flat.adt.test-fixture"),
        ),
        adt_member(
            "AuthoredOracle",
            future,
            source_fixture("liquid_ocean_flat.adt.test-fixture"),
        ),
    ]);
    fs::write(fixture.data.join("terrain.MPQ"), archive(&entries)).unwrap();
    assert_exact_inspection_without_writes(
        &fixture,
        &source_fixture("liquid_history61_positive.map"),
    );
}
