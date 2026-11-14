// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained constructor traces do not establish transport-template inputs or output.
use super::*;
#[derive(Debug, Serialize)]
pub struct Seal8TransportReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub direct_template_sites: usize,
    pub independently_matched_template_records: u64,
    pub mo_constructor_identities_observed: u64,
    pub mo_guid_contract_matches: u64,
    pub independently_matched_movement_records: u64,
    pub first_refusal: Refusal,
    pub pending: &'static [TransportPending],
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
pub fn inspect_seal8_unsealed_transports(root: &Path) -> Result<Seal8TransportReport> {
    let (held, raw) = metadata(&root.join("source-sites.json"), 1_048_576)?;
    require(
        Sha256Id::of(&raw).as_str()
            == "097b87217694b01c5a42c114358a7f30b2faf2062095640c0e50a4028e739cfa",
        "execution.transport.seal8_site_identity",
    )?;
    let sites: serde_json::Value = serde_json::from_slice(&raw)
        .map_err(|_| refuse("execution.transport.seal8_site_schema"))?;
    let direct = sites
        .as_object()
        .ok_or_else(|| refuse("execution.transport.seal8_site_schema"))?
        .values()
        .filter(|site| {
            [
                site["callsite"]["function"].as_str(),
                site["provenance"]["instrumented_file"].as_str(),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.contains("Transport"))
        })
        .count();
    let mut identities = 0;
    let mut guid_matches = 0;
    let records = super::retained_seal8::scan_seal8(root, |_, kind, payload| {
        if kind != "object_identity" {
            return Ok(());
        }
        let mut fields = payload.split_whitespace();
        let _pointer = fields.next();
        let database: u32 = super::retained_seal8::parse(fields.next())?;
        let low: u32 = super::retained_seal8::parse(fields.next())?;
        let entry: u32 = super::retained_seal8::parse(fields.next())?;
        let high: u32 = super::retained_seal8::parse(fields.next())?;
        if high == HighGuid::MoTransport as u32 {
            identities += 1;
            // Create passes entry as both database/low and zero GUID entry; Transports.cpp:161.
            require(
                database == low && entry == 0,
                "execution.transport.observed_guid_contract",
            )?;
            let actual = SourceGuid::with_entry(HighGuid::MoTransport, 0, database);
            let expected = SourceGuid::with_entry(HighGuid::MoTransport, entry, low);
            require(actual == expected, "execution.transport.observed_guid_bits")?;
            guid_matches += 1;
        }
        Ok(())
    })?;
    held.check()?;
    Ok(Seal8TransportReport {
        adapter: "benilla-seal8-08-unsealed-transport-template-coverage-v1",
        trace_sha256: Sha256Id::parse(super::retained_seal8::SEAL8_TRACE_HASH)?,
        records,
        direct_template_sites: direct,
        independently_matched_template_records: 0,
        mo_constructor_identities_observed: identities,
        mo_guid_contract_matches: guid_matches,
        independently_matched_movement_records: 0,
        first_refusal: refuse("execution.transport.sealed_definition_and_sql_occurrences_pending"),
        pending: TRANSPORT_PENDING,
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_transport_coverage_or_explicit_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path=explicit.as_ref().map(std::path::PathBuf::from).unwrap_or_else(||std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08"));
        if explicit.is_none() && !path.try_exists().unwrap() {
            println!("SKIP unsealed transport-template coverage: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = inspect_seal8_unsealed_transports(&path).unwrap();
        assert_eq!(report.direct_template_sites, 0);
        assert_eq!(report.independently_matched_template_records, 0);
        assert_eq!(report.mo_constructor_identities_observed, 9);
        assert_eq!(report.mo_guid_contract_matches, 9);
        assert_eq!(report.independently_matched_movement_records, 0);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("SKIP unsealed P0/MT transport-template comparison: no bound template loader or output observations");
        println!("{}", serde_json::to_string(&report).unwrap());
        println!("SKIP unsealed MO movement comparison: nine constructor-argument GUID projections are not independently executed constructors or captured movement state");
    }
    #[test]
    fn retained_later_transport_movement_sites_or_explicit_absence() {
        let Some(root) = std::env::var_os("BENILLA_F1_TRANSPORT_FRAMING") else {
            println!("SKIP later unsealed MT coverage: set BENILLA_F1_TRANSPORT_FRAMING");
            return;
        };
        let (held, raw) = metadata(
            &std::path::PathBuf::from(root).join("source-sites.json"),
            1_048_576,
        )
        .unwrap();
        let sites: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let movement_sites = sites
            .as_object()
            .unwrap()
            .values()
            .filter(|site| {
                let function = site["callsite"]["function"].as_str().unwrap_or("");
                function.contains("Transport::Update(")
                    || function.contains("Transport::CalculateSegmentPos(")
                    || function.contains("GenericTransport::UpdatePosition(")
            })
            .count();
        held.check().unwrap();
        println!("SKIP later unsealed MT state comparison: metadata sha256={}, movement sites={}, independently matched movement records=0", Sha256Id::of(&raw).as_str(), movement_sites);
    }
}
