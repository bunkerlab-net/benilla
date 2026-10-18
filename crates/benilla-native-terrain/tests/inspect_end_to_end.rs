// SPDX-License-Identifier: GPL-3.0-or-later
//! Run the actual helper over authored MPQs; successful inspection must never publish.
mod support;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
use support::{archive, raw_entry};
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
    fn install(&self, liquid: bool) -> Vec<u8> {
        let mut adt = include_bytes!("../tools/fixtures/flat.adt.test-fixture").to_vec();
        if liquid {
            let at = adt.windows(4).position(|v| v == b"KNCM").unwrap() + 8;
            let flags = u32::from_le_bytes(adt[at..at + 4].try_into().unwrap()) | 4;
            adt[at..at + 4].copy_from_slice(&flags.to_le_bytes());
        }
        let bytes = archive(&[
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
        ]);
        fs::write(self.data.join("terrain.MPQ"), &bytes).unwrap();
        bytes
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_benilla-native-terrain"))
            .current_dir(&self.root)
            .arg("--data")
            .arg(&self.data)
            .args(["--map", "1", "--tile", "31,32"])
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
#[test]
fn inspect_actual_preparation_is_repeatable_path_free_and_writes_nothing() {
    let fixture = Fixture::new();
    fixture.install(false);
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
fn inspect_rejects_liquid_with_the_real_writer_and_no_writes() {
    let fixture = Fixture::new();
    fixture.install(true);
    let before = snapshot(&fixture.root);
    let output = fixture.run(&["--inspect"]);
    assert!(!output.status.success());
    let text = output_text(&output);
    assert!(text.contains("terrain.liquid_excluded"), "{text}");
    assert!(!text.contains(&fixture.root.to_string_lossy().to_string()));
    assert_eq!(snapshot(&fixture.root), before);
}
#[test]
fn inspect_preserves_winner_mismatch_even_for_equal_member_bytes() {
    let fixture = Fixture::new();
    let bytes = fixture.install(false);
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
    fixture.install(false);
    let before = snapshot(&fixture.root);
    let output = fixture.run(&["--inspect", "--output", "private-output"]);
    assert!(!output.status.success());
    assert!(output_text(&output).contains("cli.mode_conflict"));
    assert_eq!(snapshot(&fixture.root), before);
}
#[test]
fn ordinary_export_still_publishes_the_same_inspected_bytes() {
    let fixture = Fixture::new();
    fixture.install(false);
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
