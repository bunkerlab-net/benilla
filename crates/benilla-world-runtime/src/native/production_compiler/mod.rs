// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw input integrity and reproducible diagnostics. This module cannot mint NativeCapability.
mod catalog;
pub use catalog::diagnose_catalog_content;
pub use catalog::diagnose_catalog_events;
pub use catalog::diagnose_catalog_groups;
pub use catalog::diagnose_catalog_preparation;
pub use catalog::diagnose_catalog_scripts;
pub use catalog::diagnose_catalog_spells;
pub(crate) use catalog::Held as ExecutionFile;
pub(crate) use catalog::PoolCatalogRows;
pub use catalog::{
    diagnose_catalog, diagnose_catalog_pools, diagnose_catalog_spawns, RetainedCatalogRow,
    RowView as CatalogRowView, SpawnViews, VerifiedCatalog,
};
pub use catalog::{diagnose_catalog_dbc, CatalogDbcView};
pub(crate) use catalog::{LinkingCatalogRows, SpawnGroupCatalogRows};
pub use catalog::{
    TransportAnimationRecord, TransportGameObject, TransportIntegrityRow, TransportSqlRow,
};
mod catalog_schema;
pub use catalog_schema::diagnose_catalog_schema;
mod dbc;
mod extensions;
mod global_hooks;
mod inputs;
pub(crate) mod json;
mod location_inputs;
pub(super) mod preparation;
mod source;
#[cfg(test)]
mod tests;
pub use preparation::loot_assembly::CheckedLootDraft;
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
    pub location: Option<&'a Path>,
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
    inputs: inputs::VerifiedProductionInputs,
}
/// Checked compiler requirements, not an app collision token or gameplay authority.
/// Phase 6 must bind the final runtime content identity and closed controller
/// before the app can construct its empty non-render participant token.
pub struct CheckedParticipantRequirements<'a> {
    report: &'a ProductionInputReport,
}
impl CheckedParticipantRequirements<'_> {
    pub fn map(&self) -> u32 {
        self.report.identity()["selection"]["map_id"]
            .as_u64()
            .expect("checked map") as u32
    }
    pub fn source_sha256(&self) -> &str {
        self.report.identity()["source_sha256"]
            .as_str()
            .expect("checked source identity")
    }
    pub fn capture_sha256(&self) -> Option<&str> {
        self.report.preparation()["participants_plan"]["capture_sha256"].as_str()
    }
    pub fn proved_empty(&self) -> bool {
        self.report.preparation()["participants_plan"]["empty_participant_obligation_discharged"]
            == true
    }
    pub fn refusals(&self) -> &[Value] {
        self.report.preparation()["participants_plan"]["refusals"]
            .as_array()
            .expect("checked participant plan")
    }
}
impl ProductionInputReport {
    pub fn identity(&self) -> &Value {
        &self.inputs.identity
    }
    pub fn preparation(&self) -> &Value {
        &self.inputs.preparation
    }
    /// Owned acquisition manifest over checked captured scope, never admission readiness.
    /// Missing constructor inputs remain explicit; no final production content digest is issued.
    pub fn requirements(&self) -> Value {
        inputs::discover_requirements(&self.inputs)
    }
    pub fn participants(&self) -> CheckedParticipantRequirements<'_> {
        CheckedParticipantRequirements { report: self }
    }
    /// Checked prototypes and loader-positioned captures, never whole-host admission.
    pub fn loot_draft(&self) -> Option<&CheckedLootDraft> {
        self.inputs.loot.as_ref()
    }
}
/// Independently recompiles diagnostics from bounded raw inputs and compares the complete report.
/// No world, store, runtime profile or capability is created, even for internally consistent inputs.
pub fn diagnose_production_inputs(
    paths: ProductionInputPaths<'_>,
    request: ProductionRequest,
) -> Result<ProductionInputReport> {
    Ok(ProductionInputReport {
        inputs: inputs::verify_production_inputs(paths, request)?,
    })
}
