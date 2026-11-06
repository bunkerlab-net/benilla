// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained constructor identities do not supply linking manager/gate inputs.
use super::*;
#[derive(Debug, Serialize)]
pub struct Seal8LinkingReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub constructor_identities_observed: u64,
    pub direct_linking_sites: usize,
    pub independently_matched_linking_records: u64,
    pub first_refusal: Refusal,
    pub pending: &'static [LinkingPending],
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
pub fn inspect_seal8_unsealed_linking(root: &Path) -> Result<Seal8LinkingReport> {
    let (held, raw) = metadata(&root.join("source-sites.json"), 1_048_576)?;
    require(
        Sha256Id::of(&raw).as_str()
            == "097b87217694b01c5a42c114358a7f30b2faf2062095640c0e50a4028e739cfa",
        "execution.linking.seal8_site_identity",
    )?;
    let sites: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|_| refuse("execution.linking.seal8_site_schema"))?;
    let sites = sites
        .as_object()
        .ok_or_else(|| refuse("execution.linking.seal8_site_schema"))?;
    let direct = sites
        .values()
        .filter(|site| {
            [
                site["callsite"]["function"].as_str(),
                site["provenance"]["instrumented_file"].as_str(),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.contains("CreatureLinking"))
        })
        .count();
    let constructors = compare_seal8_unsealed_creatures(root)?;
    held.check()?;
    Ok(Seal8LinkingReport {
        adapter: "benilla-seal8-08-unsealed-linking-coverage-v1",
        trace_sha256: constructors.trace_sha256,
        records: constructors.records,
        constructor_identities_observed: constructors.creature_identity_calls_observed,
        direct_linking_sites: direct,
        independently_matched_linking_records: 0,
        first_refusal: refuse("execution.linking.sealed_occurrence_binding_pending"),
        pending: LINKING_PENDING,
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_linking_coverage_or_explicit_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path=explicit.as_ref().map(std::path::PathBuf::from).unwrap_or_else(||
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08"));
        if explicit.is_none() && !path.try_exists().unwrap() {
            println!("SKIP unsealed linking coverage: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = inspect_seal8_unsealed_linking(&path).unwrap();
        assert_eq!(report.direct_linking_sites, 0);
        assert!(report.constructor_identities_observed > 0);
        assert_eq!(report.independently_matched_linking_records, 0);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("SKIP unsealed P0/MG/MR linking comparison: no bound linking loader/holder/gate observations");
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
