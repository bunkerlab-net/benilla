// SPDX-License-Identifier: GPL-3.0-or-later
//! Exercise the real read-only CLI body against authored captured archives.
use super::super::diagnostic;
use super::*;
use serde_json::Value;
use std::ffi::OsString;

fn arguments(path: &Path) -> Vec<OsString> {
    vec![
        "--data".into(),
        path.into(),
        "--map".into(),
        "0".into(),
        "--tile".into(),
        "31,30".into(),
        "--timeout-seconds".into(),
        "60".into(),
    ]
}
fn run_report(path: &Path) -> (String, Vec<Value>) {
    let mut bytes = Vec::new();
    diagnostic::run(&arguments(path), &mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains(&path.to_string_lossy().to_string()));
    assert!(!text.contains(ROOT));
    assert!(!text.contains("model.MPQ"));
    let rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for row in &rows {
        assert_eq!(row["gameplay_admitted"], false);
        assert_eq!(row.as_object().unwrap().len(), 13);
    }
    (text, rows)
}
#[test]
fn authored_capture_diagnostic_runs_all_phases_without_writing_install() {
    let install = Install::new(&source_height_tests::shared_entries());
    let before = install.bytes();
    let (_, rows) = run_report(&install.0);
    let metrics = &rows.iter().find(|r| r["stage"] == "assembly").unwrap()["metrics"];
    assert_eq!(
        metrics["serialized_bytes"].as_u64().unwrap(),
        metrics["tree_bytes"].as_u64().unwrap()
            + metrics["tile_bytes"].as_u64().unwrap()
            + metrics["model_bytes"].as_u64().unwrap()
    );
    let stages: Vec<_> = rows
        .iter()
        .map(|row| row["stage"].as_str().unwrap())
        .collect();
    assert_eq!(
        &stages[..6],
        &[
            "selection",
            "terrain_capture",
            "capture",
            "placement",
            "assembly",
            "validation"
        ]
    );
    assert_eq!(&stages[stages.len() - 2..], &["freshness", "complete"]);
    for row in rows.iter().filter(|r| {
        !r["stage"].as_str().unwrap().starts_with("navigation_") && r["stage"] != "complete"
    }) {
        assert_eq!(row["status"], "passed", "{row}");
    }
    assert_eq!(rows.iter().filter(|r| r["stage"] == "probe").count(), 9);
    assert_eq!(rows.last().unwrap()["metrics"]["completed_probes"], 9);
    assert_eq!(
        rows.last().unwrap()["metrics"]["completed_static_probes"],
        9
    );
    assert_eq!(rows.len(), 38);
    for row in rows.iter().filter(|r| r["stage"] == "static_probe") {
        assert_eq!(row["probe"]["kind"], "static_height");
        assert_eq!(row["probe"]["value_bits"], 7.25f32.to_bits());
        let index = row["probe"]["index"].as_u64().unwrap();
        let collision = rows
            .iter()
            .find(|r| r["stage"] == "probe" && r["probe"]["index"] == index)
            .unwrap();
        assert_eq!(row["probe"]["from_bits"], collision["probe"]["from_bits"]);
    }
    for (index, row) in rows.iter().filter(|r| r["stage"] == "probe").enumerate() {
        let dx = [0.25f32, 0.5, 0.75][index / 3];
        let dy = [0.25f32, 0.5, 0.75][index % 3];
        let expected = [
            (1.0 - dx) * (1600.0 / 3.0),
            (2.0 - dy) * (1600.0 / 3.0),
            2000.0f32,
        ]
        .map(f32::to_bits);
        assert_eq!(row["probe"]["from_bits"], serde_json::json!(expected));
        assert_eq!(row["probe"]["result"], "miss");
        assert_eq!(row["probe"]["value_bits"], (-200000f32).to_bits());
    }
    assert_eq!(install.bytes(), before);
}
#[test]
fn capture_failure_reports_blocked_phases_and_never_leaks_asset_names() {
    let mut content = source_height_tests::shared_entries();
    content.retain(|entry| entry.name != GROUP);
    let install = Install::new(&content);
    let before = install.bytes();
    let (_, rows) = run_report(&install.0);
    assert_eq!(rows[1]["stage"], "terrain_capture");
    assert_eq!(rows[1]["status"], "passed");
    assert_eq!(rows[2]["stage"], "capture");
    assert_eq!(rows[2]["status"], "failed");
    assert!(rows[2]["source_stage"].is_string());
    for row in &rows[3..rows.len() - 1] {
        assert_eq!(row["status"], "unavailable");
    }
    assert_eq!(rows.last().unwrap()["metrics"]["failed_stages"], 1);
    assert_eq!(install.bytes(), before);
}

#[test]
fn empty_install_matches_actual_player_smoke_schema() {
    // Keep this independent of the Python fixture: execute the real Rust body.
    let install = Install::new(&[]);
    fs::remove_file(install.0.join("model.MPQ")).unwrap();
    let mut args = arguments(&install.0);
    args[5] = "48,32".into();
    let mut output = Vec::new();
    diagnostic::run(&args, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let stages = [
        "selection",
        "terrain_capture",
        "capture",
        "placement",
        "assembly",
        "validation",
        "probe",
        "composition",
        "static_probe",
        "composition_freshness",
        "navigation_neighbor_0",
        "navigation_neighbor_1",
        "navigation_neighbor_2",
        "navigation_neighbor_3",
        "navigation_terrain",
        "navigation_input",
        "navigation_generation",
        "navigation_validation",
        "navigation_probe",
        "navigation_freshness",
        "freshness",
        "complete",
    ];
    assert_eq!(rows.len(), stages.len());
    for (mut row, stage) in rows.into_iter().zip(stages) {
        assert!(row
            .as_object_mut()
            .unwrap()
            .remove("elapsed_ms")
            .unwrap()
            .is_u64());
        let mut expected = serde_json::json!({"format":"benilla-collision-diagnostics-v3","stage":stage,"status":"unavailable","map":0,"tile_x":48,"tile_y":32,"code":"collision.prerequisite","source_stage":null,"metrics":{},"hashes":{},"probe":null,"gameplay_admitted":false});
        match stage {
            "selection" => {
                expected["status"] = "passed".into();
                expected["code"] = Value::Null;
            }
            "terrain_capture" => {
                expected["status"] = "failed".into();
                expected["code"] = "source.open_patch_chain".into();
            }
            "capture" => {
                expected["status"] = "failed".into();
                expected["code"] = "vmap.required_member_absent".into();
                expected["source_stage"] = "LiquidTypes".into();
                expected["metrics"] = serde_json::json!({"member_lookups":1,"read_work_bytes":0,"complete_raw_products":0,"adt_attempts":0});
            }
            "complete" => {
                expected["status"] = "failed".into();
                expected["code"] = Value::Null;
                expected["metrics"] = serde_json::json!({"completed_probes":0,"completed_static_probes":0,"completed_navigation_probes":0,"failed_stages":2});
            }
            _ => {}
        }
        assert_eq!(row, expected);
    }
    assert!(install.bytes().is_empty());
}

#[test]
fn terrain_failure_preserves_independent_collision_observations() {
    let mut content = source_height_tests::shared_entries();
    content.retain(|e| e.name != "DBFilesClient\\AreaTable.dbc");
    let install = Install::new(&content);
    let (_, rows) = run_report(&install.0);
    assert_eq!(
        rows.iter()
            .find(|r| r["stage"] == "terrain_capture")
            .unwrap()["status"],
        "failed"
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r["stage"] == "probe" && r["status"] == "passed")
            .count(),
        9
    );
    assert_eq!(
        rows.iter().find(|r| r["stage"] == "composition").unwrap()["status"],
        "unavailable"
    );
}

fn navigation_report(path: &Path, helper: Option<PathBuf>) -> Vec<Value> {
    let mut output = Vec::new();
    diagnostic::run_for_test(&arguments(path), &mut output, helper).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(!text.contains(&path.to_string_lossy().to_string()));
    assert!(!text.contains(ROOT));
    assert!(!text.contains("model.MPQ"));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn genuine_navigation_diagnostic_observes_actual_helper_and_ground_queries() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let install = Install::new(&navigation_generation_tests::ridge_entries());
    let before = install.bytes();
    let rows = navigation_report(&install.0, Some(navigation_generation_tests::helper()));
    for row in &rows {
        assert_eq!(row["gameplay_admitted"], false);
        assert_eq!(row["status"], "passed", "{row}");
    }
    assert_eq!(rows.len(), 46);
    let input = rows
        .iter()
        .find(|r| r["stage"] == "navigation_input")
        .unwrap();
    assert_eq!(input["metrics"]["solid_vertices"], 33028);
    assert_eq!(input["metrics"]["solid_triangles"], 65537);
    assert_eq!(input["metrics"]["navigation_tiles"], 1);
    let generated = rows
        .iter()
        .find(|r| r["stage"] == "navigation_generation")
        .unwrap();
    assert_eq!(generated["metrics"]["navigation_polygons"], 634);
    assert_eq!(generated["metrics"]["navigation_output_bytes"], 106252 + 28);
    assert!(generated["metrics"]["native_peak_bytes"].as_u64().unwrap() > 0);
    assert_eq!(
        rows.iter()
            .filter(|r| r["stage"] == "navigation_probe")
            .count(),
        9
    );
    assert_eq!(
        rows.last().unwrap()["metrics"]["completed_navigation_probes"],
        9
    );
    assert_eq!(install.bytes(), before);
}
#[test]
fn missing_navigation_helper_keeps_other_observations_and_input_sizing() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let install = Install::new(&navigation_generation_tests::ridge_entries());
    let before = install.bytes();
    let rows = navigation_report(&install.0, None);
    for stage in [
        "terrain_capture",
        "validation",
        "composition",
        "composition_freshness",
        "navigation_input",
        "navigation_freshness",
        "freshness",
    ] {
        assert_eq!(
            rows.iter().find(|r| r["stage"] == stage).unwrap()["status"],
            "passed"
        );
    }
    let failure = rows
        .iter()
        .find(|r| r["stage"] == "navigation_generation")
        .unwrap();
    assert_eq!(failure["status"], "failed");
    assert_eq!(failure["code"], "navigation.helper_missing");
    assert_eq!(rows.last().unwrap()["metrics"]["failed_stages"], 1);
    assert_eq!(install.bytes(), before);
}
