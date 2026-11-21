// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn checked_context_outlives_sources_and_discovers_without_creating_outputs() {
    let data = corpus();
    let fixture = &data["fixtures"][0];
    let report = {
        let input = Inputs::new(fixture);
        let before = std::fs::read_dir(&input.root).unwrap().count();
        let report = input.run().unwrap();
        assert_eq!(std::fs::read_dir(&input.root).unwrap().count(), before);
        assert!(!input.root.join("world").exists());
        assert!(!input.root.join("store").exists());
        report
    };
    let checked = &report.inputs;
    assert_eq!(checked.source, fixture["source"]);
    assert_eq!(
        json::decode(&checked.source_bytes, source::MAX_BUNDLE).unwrap(),
        checked.source
    );
    assert_eq!(
        checked.extraction_bytes,
        fixture["extraction_text"].as_str().unwrap().as_bytes()
    );
    assert_eq!(checked.rules_value, fixture["rules"]);
    assert_eq!(
        serde_json::to_value(&checked.rules).unwrap(),
        checked.rules_value
    );
    assert_eq!(
        json::decode(&checked.preparation_bytes, 128 * 1024 * 1024).unwrap(),
        *report.preparation()
    );
    for (name, table) in &checked.dbcs {
        assert_eq!(
            table.data,
            unhex(fixture["dbc_hex"][name].as_str().unwrap())
        );
        for (&id, &index) in &table.by_id {
            assert_eq!(table.get(id).unwrap().index, index);
        }
    }
    let mut requirements = report.requirements();
    assert_eq!(requirements["format"], "benilla-production-requirements-v1");
    assert_eq!(&requirements["available_input_identity"], report.identity());
    assert_eq!(requirements["world_rules"], fixture["rules"]);
    assert_eq!(
        requirements["world_rules_sha256"],
        report.identity()["world_rules_sha256"]
    );
    assert_eq!(requirements["runtime_admitted"], false);
    assert!(requirements["location"].is_null());
    assert!(requirements["raw_inputs"]["location"].is_null());
    assert!(requirements.get("content_digest").is_none());
    for required in [
        "selection_approval",
        "remaining_native_policy",
        "entry_binding",
        "client_presentation",
        "geometry_products",
        "location_products",
    ] {
        assert!(requirements["required_inputs"]
            .as_array()
            .unwrap()
            .contains(&json!(required)));
    }
    assert_eq!(
        requirements["captured_scope"]["required_source_ids"],
        fixture["source"]["required_ids"]
    );
    requirements["available_input_identity"]["runtime_admitted"] = json!(true);
    assert_eq!(
        report.requirements()["available_input_identity"]["runtime_admitted"],
        false
    );
    assert_eq!(report.preparation(), &fixture["preparation"]);
}

#[test]
fn checked_context_rejects_rehashed_report_substitution() {
    let data = corpus();
    let mut changed = data["fixtures"][0].clone();
    let report = changed["preparation"].as_object_mut().unwrap();
    report.insert("runtime_export".into(), json!({"admitted":true}));
    report.remove("preparation_sha256");
    let digest = json::digest(&changed["preparation"]);
    changed["preparation"]["preparation_sha256"] = json!(digest);
    assert_eq!(
        Inputs::new(&changed).run().unwrap_err().id,
        "compiler.preparation_mismatch"
    );
}

#[test]
fn checked_context_rejects_wrong_request_selection_and_rules() {
    let data = corpus();
    let fixture = &data["fixtures"][0];
    let mut input = Inputs::new(fixture);
    input.request.race = if input.request.race == 1 { 2 } else { 1 };
    assert_eq!(input.run().unwrap_err().id, "compiler.selection_mismatch");
    let input = Inputs::new(fixture);
    let mut selected = fixture["source"]["selection"].clone();
    selected["map_id"] = json!(65535);
    std::fs::write(
        input.root.join("selection.json"),
        json::canonical(&selected),
    )
    .unwrap();
    assert_eq!(input.run().unwrap_err().id, "compiler.selection_mismatch");
    let input = Inputs::new(fixture);
    let mut rules = fixture["rules"].clone();
    let bits = rules["combat"]["aggro_rate_bits"].as_u64().unwrap();
    rules["combat"]["aggro_rate_bits"] = json!(bits + 1);
    std::fs::write(input.root.join("rules.json"), json::canonical(&rules)).unwrap();
    assert_eq!(input.run().unwrap_err().id, "compiler.preparation_mismatch");
}

#[test]
fn checked_context_rejects_stock_preset_for_a_different_level_cap() {
    let data = corpus();
    let input = Inputs::new(&data["fixtures"][0]);
    let report = diagnose_production_inputs(
        ProductionInputPaths {
            source: &input.root.join("source.json"),
            selection: &input.root.join("selection.json"),
            dbc_directory: &input.root.join("dbc"),
            preparation: &input.root.join("preparation.json"),
            location: None,
            rules: ProductionRules::Preset(benilla_sim::world_rules::STOCK_PRESET),
        },
        input.request,
    )
    .unwrap_err();
    assert_eq!(report.id, "compiler.rules_selection_mismatch");
}
