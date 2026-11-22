// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned checked acquisition inputs, never runtime admission or a Profile builder.
use super::*;
use benilla_sim::world_rules::NativeWorldRules;

#[derive(Debug)]
pub(super) struct VerifiedProductionInputs {
    pub(super) source: Value,
    pub(super) source_bytes: Vec<u8>,
    pub(super) selection: source::Selection,
    pub(super) selection_bytes: Vec<u8>,
    pub(super) profiles: source::Profiles,
    pub(super) request: ProductionRequest,
    pub(super) rules: NativeWorldRules,
    pub(super) rules_value: Value,
    pub(super) rules_bytes: Option<Vec<u8>>,
    pub(super) dbcs: dbc::Tables,
    pub(super) extraction_bytes: Vec<u8>,
    pub(super) location: Option<location_inputs::CheckedLocation>,
    pub(super) preparation_bytes: Vec<u8>,
    pub(super) preparation: Value,
    pub(super) identity: Value,
    pub(super) loot: Option<CheckedLootDraft>,
}

pub(super) fn verify_production_inputs(
    paths: ProductionInputPaths<'_>,
    request: ProductionRequest,
) -> Result<VerifiedProductionInputs> {
    request.validate()?;
    let source_bytes = json::read_regular(paths.source, source::MAX_BUNDLE)?;
    let source = json::decode(&source_bytes, source::MAX_BUNDLE)?;
    let (selection, profiles) = source::validate(&source)?;
    check(selection.starter.is_some(), "compiler.source_v2_required")?;
    let selection_bytes = json::read_regular(paths.selection, 65536)?;
    let selected = json::decode(&selection_bytes, 65536)?;
    let selected = source::Selection::parse(&selected)?;
    check(
        selected.json() == selection.json(),
        "compiler.selection_mismatch",
    )?;
    let (race, class, cap) = selection.starter.unwrap();
    check(
        race == request.race as i128 && class == request.class as i128,
        "compiler.selection_mismatch",
    )?;
    let rules_bytes = match paths.rules {
        ProductionRules::Json(path) => Some(json::read_regular(path, 65536)?),
        ProductionRules::Preset(name) => {
            check(
                name == benilla_sim::world_rules::STOCK_PRESET,
                "world_rules.unknown_preset",
            )?;
            None
        }
    };
    let rules_value = match &rules_bytes {
        Some(bytes) => json::decode(bytes, 65536)?,
        None => serde_json::to_value(NativeWorldRules::cmangos_classic_stock())
            .expect("rules serialization"),
    };
    let rules = preparation::validate_rules(&rules_value)?;
    check(
        rules.progression.max_player_level as i128 == cap,
        "compiler.rules_selection_mismatch",
    )?;
    let (dbcs, extraction, extraction_bytes) = dbc::load(paths.dbc_directory)?;
    let location = paths
        .location
        .map(|path| location_inputs::load(path, &source, &dbcs))
        .transpose()?;
    let preparation = preparation::compile(
        &source,
        &dbcs,
        request,
        extraction,
        &rules_value,
        location.as_ref(),
    )?;
    let preparation_bytes = json::read_regular(paths.preparation, 128 * 1024 * 1024)?;
    let provided = json::decode(&preparation_bytes, 128 * 1024 * 1024)?;
    check(provided == preparation, "compiler.preparation_mismatch")?;
    let mut identity = json!({"format":"benilla-production-input-identity-v1","compiler_policy":"benilla-production-diagnostics-v1","source_sha256":preparation["source_sha256"],"selection":selection.json(),"request":request.json(),"world_rules_sha256":json::digest(&rules_value),"extraction_manifest":preparation["extraction_manifest"],"dbc_manifest":preparation["dbc_manifest"],"preparation_sha256":preparation["preparation_sha256"],"client_build_authenticity_verified":false,"revision_claims_verified_against_installation":false,"consistency":source["consistency"],"runtime_admitted":false});
    identity["identity_sha256"] = json!(json::digest(&identity));
    let loot = preparation::loot_assembly::assemble(&preparation["corpse_loot_plan"])?;
    Ok(VerifiedProductionInputs {
        source,
        source_bytes,
        selection,
        selection_bytes,
        profiles,
        request,
        rules,
        rules_value,
        rules_bytes,
        dbcs,
        extraction_bytes,
        location,
        preparation_bytes,
        preparation,
        identity,
        loot,
    })
}

fn raw_binding(bytes: &[u8]) -> Value {
    json!({"sha256":json::hash_bytes(bytes),"bytes":bytes.len()})
}

/// Discovery describes only checked captured scope, not complete future gameplay requirements.
pub(super) fn discover_requirements(inputs: &VerifiedProductionInputs) -> Value {
    let location = inputs.location.as_ref();
    let mut required_inputs = vec![
        "selection_approval",
        "remaining_native_policy",
        "entry_binding",
        "client_presentation",
        "geometry_products",
    ];
    if location.is_none() {
        required_inputs.push("location_products");
    }
    let location_files = location.map(|location| {
        location
            .raw_files
            .iter()
            .map(|(name, bytes)| (name.clone(), raw_binding(bytes)))
            .collect::<serde_json::Map<_, _>>()
    });
    let dbc_files: serde_json::Map<_, _> = inputs
        .dbcs
        .iter()
        .map(|(name, table)| (name.clone(), table.manifest()))
        .collect();
    let selected_dbcs: Vec<_> = inputs.preparation["dbc_bindings"]
        .as_array().expect("checked DBC bindings").iter()
        .map(|row| json!({"table":row["table"],"key":row["key"],"record_sha256":row["record_sha256"]}))
        .collect();
    json!({
        "format":"benilla-production-requirements-v1",
        "available_input_identity":inputs.identity,
        "selection":inputs.selection.json(),
        "request":inputs.request.json(),
        "location":location.map(|value| &value.report),
        "required_inputs":required_inputs,
        "captured_scope":{
            "required_source_ids":inputs.source["required_ids"],
            "schema_tables":inputs.profiles.keys().collect::<Vec<_>>(),
            "selected_dbc_bindings":selected_dbcs,
            "participants_capture_sha256":inputs.preparation["participants_plan"]["capture_sha256"],
        },
        "raw_inputs":{
            "source":raw_binding(&inputs.source_bytes),
            "selection":raw_binding(&inputs.selection_bytes),
            "rules":inputs.rules_bytes.as_deref().map(raw_binding),
            "extraction":raw_binding(&inputs.extraction_bytes),
            "preparation":raw_binding(&inputs.preparation_bytes),
            "dbcs":dbc_files,
            "location":location_files,
        },
        "world_rules":inputs.rules,
        "world_rules_sha256":json::digest(&inputs.rules_value),
        "runtime_admitted":false,
    })
}
