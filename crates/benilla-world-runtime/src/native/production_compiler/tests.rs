// SPDX-License-Identifier: GPL-3.0-or-later
#[path = "limits_tests.rs"]
mod limits_tests;
use super::*;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
const CASES: &str =
    include_str!("../../../../../scripts/native_starter/production_verifier_cases.json");
fn corpus() -> Value {
    let mut value = json::decode(CASES.as_bytes(), 8 * 1024 * 1024).unwrap();
    value["fixtures"].as_array_mut().unwrap().extend(
        combat_corpus()["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    value
}
fn combat_corpus() -> Value {
    json::decode(
        include_bytes!("../../../../../scripts/native_starter/combat_world_rules_cases.json"),
        1024 * 1024,
    )
    .unwrap()
}
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
struct Inputs {
    root: PathBuf,
    request: ProductionRequest,
}
impl Inputs {
    fn new(fixture: &Value) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "benilla-production-verifier-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("dbc")).unwrap();
        for (name, bytes) in fixture["dbc_hex"].as_object().unwrap() {
            std::fs::write(
                root.join("dbc").join(format!("{name}.dbc")),
                unhex(bytes.as_str().unwrap()),
            )
            .unwrap();
        }
        for (name, value) in [
            ("source.json", &fixture["source"]),
            ("selection.json", &fixture["source"]["selection"]),
            ("rules.json", &fixture["rules"]),
            ("preparation.json", &fixture["preparation"]),
        ] {
            std::fs::write(root.join(name), json::canonical(value)).unwrap();
        }
        std::fs::write(
            root.join("dbc/extraction.json"),
            fixture["extraction_text"].as_str().unwrap(),
        )
        .unwrap();
        Self {
            root,
            request: ProductionRequest {
                race: fixture["request"]["race"].as_u64().unwrap() as u8,
                class: fixture["request"]["class"].as_u64().unwrap() as u8,
                gender: fixture["request"]["gender"].as_u64().unwrap() as u8,
            },
        }
    }
    fn run(&self) -> Result<ProductionInputReport> {
        diagnose_production_inputs(
            ProductionInputPaths {
                source: &self.root.join("source.json"),
                selection: &self.root.join("selection.json"),
                dbc_directory: &self.root.join("dbc"),
                preparation: &self.root.join("preparation.json"),
                rules: ProductionRules::Json(&self.root.join("rules.json")),
            },
            self.request,
        )
    }
}
impl Drop for Inputs {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            let keys = || a.keys().chain(b.keys());
            for key in keys()
                .filter(|key| !key.ends_with("_sha256"))
                .chain(keys().filter(|key| key.ends_with("_sha256")))
            {
                match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => {
                        if let Some(p) = difference(a, b, &format!("{path}/{key}")) {
                            return Some(p);
                        }
                    }
                    _ => return Some(format!("{path}/{key}: missing field")),
                }
            }
            None
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!("{path}: lengths {} != {}", a.len(), b.len()));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .find_map(|(i, (a, b))| difference(a, b, &format!("{path}/{i}")))
        }
        _ => {
            if a == b {
                None
            } else {
                Some(format!("{path}: {a} != {b}"))
            }
        }
    }
}

#[test]
fn production_verifier_shared_mutations_have_exact_diagnostic_ids() {
    let fixtures = corpus();
    let mut cases = json::decode(
        include_bytes!("../../../../../scripts/native_starter/production_verifier_mutations.json"),
        4 * 1024 * 1024,
    )
    .unwrap();
    cases["cases"]
        .as_array_mut()
        .unwrap()
        .extend(combat_corpus()["cases"].as_array().unwrap().iter().cloned());
    for case in cases["cases"].as_array().unwrap() {
        let mut fixture = fixtures["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == case["fixture"])
            .unwrap()
            .clone();
        let value = &mut fixture[case["target"].as_str().unwrap()];
        for operation in case["operations"].as_array().unwrap() {
            let path = operation["path"].as_str().unwrap();
            if operation["op"] == "append" {
                value
                    .pointer_mut(path)
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .push(operation["value"].clone());
                continue;
            }
            let (parent, key) = path.rsplit_once('/').unwrap();
            let parent = value.pointer_mut(parent).unwrap();
            if operation["op"] == "remove" {
                if let Some(array) = parent.as_array_mut() {
                    array.remove(key.parse::<usize>().unwrap());
                } else {
                    parent.as_object_mut().unwrap().remove(key);
                }
            } else if let Some(array) = parent.as_array_mut() {
                array[key.parse::<usize>().unwrap()] = operation["value"].clone();
            } else {
                parent[key] = operation["value"].clone();
            }
        }
        for binding in case["bindings"].as_array().unwrap() {
            let digest = json::digest(value.pointer(binding["of"].as_str().unwrap()).unwrap());
            let (parent, key) = binding["path"].as_str().unwrap().rsplit_once('/').unwrap();
            value.pointer_mut(parent).unwrap()[key] = json!(digest);
        }
        let inputs = Inputs::new(&fixture);
        assert_eq!(
            inputs.run().unwrap_err().id,
            case["diagnostic"].as_str().unwrap(),
            "case {}",
            case["name"]
        );
    }
}
#[test]
fn production_verifier_matches_complete_python_preparation() {
    for fixture in corpus()["fixtures"].as_array().unwrap() {
        let source = &fixture["source"];
        source::validate(source).unwrap_or_else(|e| panic!("{}: {e}", fixture["name"]));
        let tables: BTreeMap<_, _> = fixture["dbc_hex"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, data)| {
                (
                    name.clone(),
                    dbc::Table::parse(name, unhex(data.as_str().unwrap())).unwrap(),
                )
            })
            .collect();
        let preparation = preparation::compile(
            source,
            &tables,
            ProductionRequest {
                race: fixture["request"]["race"].as_u64().unwrap() as u8,
                class: fixture["request"]["class"].as_u64().unwrap() as u8,
                gender: fixture["request"]["gender"].as_u64().unwrap() as u8,
            },
            fixture["preparation"]["extraction_manifest"].clone(),
            &fixture["rules"],
        )
        .unwrap();
        assert_eq!(
            difference(&preparation, &fixture["preparation"], ""),
            None,
            "fixture {}",
            fixture["name"]
        );
        let inputs = Inputs::new(fixture);
        let report = inputs.run().unwrap();
        assert_eq!(report.preparation()["status"], "blocked");
        assert_eq!(report.identity()["runtime_admitted"], false);
        assert_eq!(
            report.identity()["client_build_authenticity_verified"],
            false
        );
        assert_eq!(inputs.run().unwrap().identity(), report.identity());
    }
}
#[test]
fn production_verifier_rejects_rehashed_preparation_after_blocker_removal() {
    let data = corpus();
    let fixture = &data["fixtures"][0];
    let inputs = Inputs::new(fixture);
    let mut report = fixture["preparation"].clone();
    report["unsupported"].as_array_mut().unwrap().pop();
    report.as_object_mut().unwrap().remove("preparation_sha256");
    report["preparation_sha256"] = json!(json::digest(&report));
    std::fs::write(
        inputs.root.join("preparation.json"),
        json::canonical(&report),
    )
    .unwrap();
    assert_eq!(
        inputs.run().unwrap_err().id,
        "compiler.preparation_mismatch"
    );
}
#[test]
fn production_verifier_strict_json_rejects_duplicates_nonfinite_depth_and_unknown_fields() {
    let cases = malformed_cases();
    for case in cases["json"].as_array().unwrap() {
        assert_eq!(
            json::decode(case["raw"].as_str().unwrap().as_bytes(), 1024)
                .unwrap_err()
                .id,
            case["diagnostic"].as_str().unwrap(),
        );
    }
    assert_eq!(json::decode(b"null", 3).unwrap_err().id, "json.byte_limit");
    let mut source = corpus()["fixtures"][0]["source"].clone();
    source["unknown"] = json!(0);
    assert_eq!(source::validate(&source).unwrap_err().id, "bundle.fields");
}
#[test]
fn production_verifier_exact_binary32_matches_authored_ties_and_subnormals() {
    for case in malformed_cases()["binary32"].as_array().unwrap() {
        assert_eq!(
            dbc::binary32(case["decimal"].as_str().unwrap()).unwrap() as u64,
            case["bits"].as_u64().unwrap(),
            "{}",
            case["decimal"],
        );
    }
    assert_eq!(
        dbc::binary32("1e100").unwrap_err().id,
        "numeric.binary32_overflow"
    );
}

#[test]
fn production_verifier_decimal_comparisons_preserve_sub_float_boundaries() {
    let cases = json::decode(
        include_bytes!("../../../../../scripts/native_starter/decimal_parity.json"),
        4096,
    )
    .unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let order = source::compare_decimal(
            case["left"].as_str().unwrap(),
            case["right"].as_str().unwrap(),
        );
        assert_eq!(order as i8, case["order"].as_i64().unwrap() as i8, "{case}");
    }
}

fn malformed_cases() -> Value {
    json::decode(
        include_bytes!("../../../../../scripts/native_starter/production_verifier_malformed.json"),
        4 * 1024 * 1024,
    )
    .unwrap()
}

#[test]
fn production_verifier_shared_wdbc_malformed_cases_have_stable_ids() {
    for case in malformed_cases()["dbc"].as_array().unwrap() {
        let result = dbc::Table::parse(
            case["table"].as_str().unwrap(),
            unhex(case["hex"].as_str().unwrap()),
        );
        assert_eq!(
            result.err().unwrap().id,
            case["diagnostic"].as_str().unwrap(),
            "case {}",
            case["name"]
        );
    }
}

#[test]
fn production_verifier_binds_whole_dbc_and_exact_extraction_bytes() {
    let data = corpus();
    let fixture = &data["fixtures"][0];
    // A valid, otherwise unconsumed DBC byte still changes the whole-file binding.
    let inputs = Inputs::new(fixture);
    let mut bytes = unhex(fixture["dbc_hex"]["Map"].as_str().unwrap());
    bytes[20 + 3 * 4] ^= 1;
    std::fs::write(inputs.root.join("dbc/Map.dbc"), bytes).unwrap();
    assert_eq!(
        inputs.run().unwrap_err().id,
        "extraction.dbc_manifest_mismatch"
    );
    // Whitespace is semantically inert JSON but is part of extraction.json's identity.
    let inputs = Inputs::new(fixture);
    std::fs::write(
        inputs.root.join("dbc/extraction.json"),
        format!("{} ", fixture["extraction_text"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(
        inputs.run().unwrap_err().id,
        "compiler.preparation_mismatch"
    );
    // Labels never become paths, and fixed basenames must be unique.
    for (field, invalid, expected) in [
        (
            "winning_archive",
            json!("../dbc.MPQ"),
            "extraction.archive_label",
        ),
        ("file", json!("../Map.dbc"), "extraction.manifest_file_set"),
        ("bytes", json!(1), "extraction.dbc_manifest_mismatch"),
        ("records", json!(1.0), "extraction.dbc_manifest_mismatch"),
    ] {
        let inputs = Inputs::new(fixture);
        let mut manifest: Value =
            serde_json::from_str(fixture["extraction_text"].as_str().unwrap()).unwrap();
        manifest["files"][0][field] = invalid;
        std::fs::write(
            inputs.root.join("dbc/extraction.json"),
            json::canonical(&manifest),
        )
        .unwrap();
        assert_eq!(inputs.run().unwrap_err().id, expected);
    }
    for format in [
        "benilla-native-dbc-extraction-v1",
        "benilla-native-dbc-extraction-v2",
        "benilla-native-dbc-extraction-v3",
    ] {
        let inputs = Inputs::new(fixture);
        let mut manifest: Value =
            serde_json::from_str(fixture["extraction_text"].as_str().unwrap()).unwrap();
        manifest["format"] = json!(format);
        std::fs::write(
            inputs.root.join("dbc/extraction.json"),
            json::canonical(&manifest),
        )
        .unwrap();
        let expected = if format.ends_with("v3") {
            "extraction.complete_valid_dbc_set_required"
        } else {
            "extraction.legacy_manifest_reextract_v3_required"
        };
        assert_eq!(inputs.run().unwrap_err().id, expected);
    }
}
#[cfg(any(unix, windows))]
#[test]
fn production_verifier_rejects_symlinks_fifos_and_devices_without_waiting() {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as symlink;
    let data = corpus();
    let inputs = Inputs::new(&data["fixtures"][0]);
    let original = inputs.root.join("source.json");
    let link = inputs.root.join("link.json");
    let linked = symlink(&original, &link);
    #[cfg(unix)]
    linked.unwrap();
    #[cfg(windows)]
    if let Err(error) = linked {
        // ERROR_PRIVILEGE_NOT_HELD. Still exercise refusal without symlink privileges.
        // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createsymboliclinkw
        assert_eq!(error.raw_os_error(), Some(1314));
        std::fs::create_dir(&link).unwrap();
    }
    assert_eq!(
        json::read_regular(&link, 1024).unwrap_err().id,
        "input.regular_file_required"
    );
    #[cfg(unix)]
    {
        let fifo = inputs.root.join("fifo");
        let result = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(result.success());
        assert_eq!(
            json::read_regular(&fifo, 1024).unwrap_err().id,
            "input.regular_file_required"
        );
        assert_eq!(
            json::read_regular(Path::new("/dev/null"), 1024)
                .unwrap_err()
                .id,
            "input.regular_file_required"
        );
    }
    #[cfg(windows)]
    {
        let (pipe, _server) = named_pipe(&inputs);
        assert_eq!(
            json::read_regular(&pipe, 1024).unwrap_err().id,
            "input.regular_file_required"
        );
        assert_eq!(
            json::read_regular(Path::new(r"\\.\NUL"), 1024)
                .unwrap_err()
                .id,
            "input.regular_file_required"
        );
    }
    assert_eq!(
        json::read_regular(&inputs.root, 1024).unwrap_err().id,
        "input.regular_file_required"
    );
}

#[cfg(windows)]
fn named_pipe(
    inputs: &Inputs,
) -> (
    PathBuf,
    (
        tokio::net::windows::named_pipe::NamedPipeServer,
        tokio::runtime::Runtime,
    ),
) {
    use tokio::{net::windows::named_pipe::ServerOptions, runtime::Builder};
    let path = PathBuf::from(format!(
        r"\\.\pipe\{}",
        inputs.root.file_name().unwrap().to_str().unwrap()
    ));
    let runtime = Builder::new_current_thread().enable_io().build().unwrap();
    let server = {
        let _entered = runtime.enter();
        ServerOptions::new()
            .access_inbound(false)
            .access_outbound(true)
            .max_instances(1)
            .out_buffer_size(1024)
            .create(&path)
            .unwrap()
    };
    (path, (server, runtime))
}

#[test]
fn initial_execution_shapes_agree_with_existing_runtime_phase_classifier() {
    use benilla_sim::starter_init::{
        initial_cast_phases::{
            AuthoredInitialCast, AuthoredPhasePrerequisites, AuthoredPhaseResolutions,
            InitialCastPlan,
        },
        Rows, SpellRow,
    };
    let corpus = corpus();
    for (name, supported) in [
        ("initial-spell-execution", true),
        ("initial-spell-execution-refusals", false),
    ] {
        let fixture = corpus["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap();
        let mut checked = 0;
        for spell in fixture["preparation"]["spells"].as_array().unwrap() {
            if spell["execution_plan"]["classification"] != "existing_initial_phase_handlers" {
                continue;
            }
            checked += 1;
            assert_eq!(
                spell["execution_plan"]["status"] == "supported_shape",
                supported
            );
            let words =
                std::array::from_fn(|i| spell["server_numeric_words"][i].as_u64().unwrap() as u32);
            let row = &spell["numeric_capture"]["row"];
            let coefficients = |name: &str| {
                std::array::from_fn(|i| {
                    dbc::binary32(row[format!("{name}{}", i + 1)]["decimal"].as_str().unwrap())
                        .unwrap()
                })
            };
            let duration_ms = spell["duration"]["base_ms"].as_i64().unwrap_or(0) as i32;
            let cast = AuthoredInitialCast {
                row: SpellRow {
                    id: words[0],
                    words,
                    duration_ms: Some(duration_ms),
                    previous_rank: None,
                    talent: false,
                    learned_spells: Rows::Complete(vec![]),
                    external_initial_hooks: false,
                },
                prerequisites: Some(AuthoredPhasePrerequisites {
                    fresh_unshapeshifted_player_not_in_world: true,
                    successful_conditions_and_location_checks: true,
                    spell_and_aura_scripts_absent: true,
                    spell_modifiers_absent: true,
                    neutral_proc_and_finish_paths: true,
                    server_side: row["IsServerSide"].as_u64().unwrap() as u32,
                    server_attributes: row["AttributesServerside"].as_u64().unwrap() as u32,
                    effect_bonus_coefficient_bits: coefficients("EffectBonusCoefficient"),
                    effect_bonus_coefficient_from_ap_bits: coefficients(
                        "EffectBonusCoefficientFromAP",
                    ),
                }),
                resolutions: Some(AuthoredPhaseResolutions {
                    cast_time_index: words[18],
                    cast_time_ms: 0,
                    duration_index: words[30],
                    duration_ms,
                }),
            };
            assert_eq!(
                InitialCastPlan::from_authored(&[cast]).is_ok(),
                supported,
                "existing runtime classifier disagrees for {}",
                words[0]
            );
        }
        assert_eq!(checked, 15);
    }
}
