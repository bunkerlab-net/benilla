// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw input integrity and reproducible diagnostics. This module cannot mint NativeCapability.
mod dbc;
mod extensions;
mod json;
mod preparation;
mod source;
#[cfg(test)]
mod tests;
use serde_json::{json, Value};
use std::{path::Path, sync::LazyLock};
static COMPILER_CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("compiler_contract.json"))
        .expect("reviewed compiler metadata")
});
type Result<T, E = ProductionDiagnostic> = std::result::Result<T, E>;

/// Stable IDs contain no imported text, paths, credentials or OS exception details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionDiagnostic {
    pub id: String,
}
impl std::fmt::Display for ProductionDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id)
    }
}
impl std::error::Error for ProductionDiagnostic {}
fn fail(id: &str) -> ProductionDiagnostic {
    ProductionDiagnostic { id: id.into() }
}
fn check(ok: bool, id: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(fail(id))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductionRequest {
    pub race: u8,
    pub class: u8,
    pub gender: u8,
}
impl ProductionRequest {
    fn json(self) -> Value {
        json!({"race":self.race,"class":self.class,"gender":self.gender})
    }
    fn validate(self) -> Result<()> {
        check((1..=8).contains(&self.race), "request.race")?;
        check(
            [1, 2, 3, 4, 5, 7, 8, 9, 11].contains(&self.class),
            "request.class",
        )?;
        check(self.gender <= 1, "request.gender")
    }
}
/// Every input is local and explicit. The selection file is compared with the source selection.
pub struct ProductionInputPaths<'a> {
    pub source: &'a Path,
    pub selection: &'a Path,
    pub dbc_directory: &'a Path,
    pub preparation: &'a Path,
    pub rules: ProductionRules<'a>,
}
pub enum ProductionRules<'a> {
    Json(&'a Path),
    Preset(&'a str),
}
/// Integrity is not admission, authenticity, source ownership, loaded order or executable support.
/// The complete preparation retains its original findings and nested refusals.
#[derive(Debug)]
pub struct ProductionInputReport {
    identity: Value,
    preparation: Value,
}
impl ProductionInputReport {
    pub fn identity(&self) -> &Value {
        &self.identity
    }
    pub fn preparation(&self) -> &Value {
        &self.preparation
    }
}
/// Independently recompiles diagnostics from bounded raw inputs and compares the complete report.
/// No world, store, runtime profile or capability is created, even for internally consistent inputs.
pub fn diagnose_production_inputs(
    paths: ProductionInputPaths<'_>,
    request: ProductionRequest,
) -> Result<ProductionInputReport> {
    request.validate()?;
    let source = json::decode(
        &json::read_regular(paths.source, source::MAX_BUNDLE)?,
        source::MAX_BUNDLE,
    )?;
    let (selection, _profiles) = source::validate(&source)?;
    check(selection.starter.is_some(), "compiler.source_v2_required")?;
    let selected = json::decode(&json::read_regular(paths.selection, 65536)?, 65536)?;
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
    let rules = match paths.rules {
        ProductionRules::Json(path) => json::decode(&json::read_regular(path, 65536)?, 65536)?,
        ProductionRules::Preset(name) => {
            check(
                name == benilla_sim::world_rules::STOCK_PRESET,
                "world_rules.unknown_preset",
            )?;
            serde_json::to_value(
                benilla_sim::world_rules::NativeWorldRules::cmangos_classic_stock(),
            )
            .expect("rules serialization")
        }
    };
    preparation::validate_rules(&rules)?;
    check(
        json::int(&rules["progression"]["max_player_level"]) == cap,
        "compiler.rules_selection_mismatch",
    )?;
    let (dbcs, extraction) = dbc::load(paths.dbc_directory)?;
    let preparation = preparation::compile(&source, &dbcs, request, extraction, &rules)?;
    let provided = json::decode(
        &json::read_regular(paths.preparation, 128 * 1024 * 1024)?,
        128 * 1024 * 1024,
    )?;
    check(provided == preparation, "compiler.preparation_mismatch")?;
    let mut identity = json!({"format":"benilla-production-input-identity-v1","compiler_policy":"benilla-production-diagnostics-v1","source_sha256":preparation["source_sha256"],"selection":selection.json(),"request":request.json(),"world_rules_sha256":json::digest(&rules),"extraction_manifest":preparation["extraction_manifest"],"dbc_manifest":preparation["dbc_manifest"],"preparation_sha256":preparation["preparation_sha256"],"client_build_authenticity_verified":false,"revision_claims_verified_against_installation":false,"consistency":source["consistency"],"runtime_admitted":false});
    identity["identity_sha256"] = json!(json::digest(&identity));
    Ok(ProductionInputReport {
        identity,
        preparation,
    })
}
