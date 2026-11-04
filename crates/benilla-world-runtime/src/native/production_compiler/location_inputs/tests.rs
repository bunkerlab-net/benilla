// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn raw_location_bytes_match_python_and_refuse_rebound_mutations() {
    let cases = json::decode(
        include_bytes!("../../../../../../scripts/native_starter/source_location_cases.json"),
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(cases["format"], "benilla-source-location-reader-cases-v1");
    assert_eq!(cases["cases"].as_array().unwrap().len(), 11);
    let corpus = corpus();
    let inputs = Inputs::new(&corpus["fixtures"][0]);
    let mut checked = 0;
    for case in cases["cases"].as_array().unwrap() {
        for (name, data) in case["files"].as_object().unwrap() {
            let path = inputs.root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, unhex(data.as_str().unwrap())).unwrap();
        }
        let tables = BTreeMap::from([(
            "Map".into(),
            dbc::Table::parse("Map", unhex(case["map_dbc"].as_str().unwrap())).unwrap(),
        )]);
        let result = super::super::location_inputs::load(
            &inputs.root.join("input.json"),
            &case["source"],
            &tables,
        );
        if let Some(expected) = case["diagnostic"].as_str() {
            assert_eq!(
                result.expect_err("mutation must refuse").id,
                expected,
                "{}",
                case["name"]
            );
        } else {
            let checked_location = result.unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
            assert_eq!(checked_location.report, case["result"], "{}", case["name"]);
            for (name, bytes) in &checked_location.raw_files {
                assert_eq!(
                    bytes.as_slice(),
                    unhex(case["files"][name].as_str().unwrap())
                );
                std::fs::write(inputs.root.join(name), b"replaced after verification").unwrap();
                assert_eq!(
                    bytes.as_slice(),
                    unhex(case["files"][name].as_str().unwrap())
                );
            }
            assert!(!checked_location.raw_files.is_empty());
            let narrowed = case["name"] == "narrowed-flags";
            assert_eq!(
                (checked_location.area_id, checked_location.zone_id),
                if narrowed { (12, 12) } else { (9, 12) }
            );
            assert_eq!(
                checked_location.report["area_flag"],
                if narrowed { 1 } else { 125 }
            );
            assert_eq!(checked_location.report["runtime_export"], Value::Null);
            checked += 1;
        }
    }
    assert_eq!(checked, 2);
}
