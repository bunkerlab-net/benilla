// SPDX-License-Identifier: GPL-3.0-or-later
//! Run the actual helper over authored MPQs; successful inspection must never publish.
#[path = "support/map_dbc.rs"]
mod map_fixture;
mod support;
use benilla_native_terrain::{prepare, Tile};
use map_fixture::map_dbc;
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
fn inspect_refuses_history_dependent_liquid_without_leaking_paths_or_writing() {
    for case in ["liquid_stale_edge_a", "liquid_stale_edge_b"] {
        let fixture = Fixture::new();
        fixture.install(case);
        assert_rejected_without_writes(&fixture, "terrain.liquid_history");
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
    assert_eq!(manifest["format"], "benilla-terrain-production-v2");
    assert_eq!(
        manifest["writer_policy"],
        "cmangos-ad-f0-selected-tile-owned-liquid-v2"
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
