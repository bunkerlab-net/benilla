// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained lock arithmetic fixtures do not author active membership or grid lifetimes.
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
pub struct Seal8ActiveDifference {
    pub reference_event: u64,
    pub operation: String,
    pub expected_count: u16,
    pub fixture_count: u16,
    pub owner: &'static str,
    pub reason: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Seal8ActiveReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub active_add_records_observed: u64,
    pub lock_increments_observed: u64,
    pub lock_decrements_observed: u64,
    pub explicit_lock_records_observed: u64,
    pub relocation_named_records_observed: u64,
    pub lock_arithmetic_records_matched: u64,
    pub independently_executed_active_changes_matched: u64,
    pub first_difference: Option<Seal8ActiveDifference>,
    pub pending: Vec<&'static str>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
fn pointer(field: Option<&str>) -> Result<u64> {
    u64::from_str_radix(
        field
            .and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| refuse("execution.seal8_active_pointer"))?,
        16,
    )
    .map_err(|_| refuse("execution.seal8_active_pointer"))
}
// Initial GridInfo counts are zero; pointer reuse without a bound grid lifetime stops this fixture; NGrid.h:35-64.
pub fn compare_seal8_unsealed_active(root: &Path) -> Result<Seal8ActiveReport> {
    let mut grids = SourceGrids::new(MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    })?;
    grids.configure(300000, true)?;
    let mut identities = BTreeMap::<u64, GridIdentity>::new();
    let (mut adds, mut inc, mut dec, mut explicit, mut relocations, mut matched) =
        (0, 0, 0, 0, 0, 0);
    let mut difference = None;
    let records = scan_seal8(root, |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "active_add" => {
                let _: u32 = parse(fields.next())?;
                let _ = pointer(fields.next())?;
                let _: u32 = parse(fields.next())?;
                adds += 1;
            }
            "grid_lock_explicit" => {
                let _ = pointer(fields.next())?;
                let flag: u8 = parse(fields.next())?;
                require(flag <= 1, "execution.seal8_explicit_lock_boolean")?;
                explicit += 1;
            }
            "grid_lock_inc" | "grid_lock_dec" => {
                let pointer = pointer(fields.next())?;
                let expected: u16 = parse(fields.next())?;
                if kind == "grid_lock_inc" {
                    inc += 1;
                } else {
                    dec += 1;
                }
                if difference.is_some() {
                    return Ok(());
                }
                let grid = if let Some(grid) = identities.get(&pointer) {
                    *grid
                } else {
                    let index = identities.len();
                    require(index < 4096, "execution.seal8_active_lock_fixture_bound")?;
                    let grid = grids.create((index % 64) as u16, (index / 64) as u16)?;
                    identities.insert(pointer, grid);
                    grid
                };
                let actual = grids.change_active_lock(grid, kind == "grid_lock_inc")?;
                if expected == actual {
                    matched += 1;
                } else {
                    difference = Some(Seal8ActiveDifference {
                        reference_event: context.event,
                        operation: kind.into(),
                        expected_count: expected,
                        fixture_count: actual,
                        owner: "NGrid.h:53-64",
                        reason:
                            "lock arithmetic differs under an unbound GridInfo pointer lifetime",
                    });
                }
            }
            _ => {
                if kind.contains("relocat") {
                    relocations += 1;
                }
            }
        }
        Ok(())
    })?;
    Ok(Seal8ActiveReport {
        adapter: "benilla-seal8-08-unsealed-active-lock-fixtures-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records,
        active_add_records_observed: adds,
        lock_increments_observed: inc,
        lock_decrements_observed: dec,
        explicit_lock_records_observed: explicit,
        relocation_named_records_observed: relocations,
        lock_arithmetic_records_matched: matched,
        independently_executed_active_changes_matched: 0,
        first_difference: difference,
        pending: vec![
            "GridInfo allocation and grid-generation joins",
            "Independent constructor and active-state causes",
            "Complete remove and iterator mutation observations",
            "Actual preload occurrence and forced-map container order",
            "Authoritative poses and respawn definition lookups",
            "Relocation and viewpoint handler observations",
            "Natural unload/reload and remote actor scenario",
            "Sealed reference execution",
        ],
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_seal8_active_lock_projection_or_clean_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path =
            explicit
                .clone()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap()
            .join("benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08")
                });
        if explicit.is_none() && !path.try_exists().unwrap() {
            eprintln!(
                "SKIP unsealed seal8-08 active/lock projection: set BENILLA_F1_SEAL8_FRAMING"
            );
            return;
        }
        let report = compare_seal8_unsealed_active(&path).unwrap();
        assert!(report.active_add_records_observed > 0 && report.lock_increments_observed > 0);
        assert_eq!(report.independently_executed_active_changes_matched, 0);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
