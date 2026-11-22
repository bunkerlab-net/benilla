// SPDX-License-Identifier: GPL-3.0-or-later
//! Constructor observations and GUID-only comparisons, never captured GO gameplay inputs.
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct Seal8GameObjectReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub ordinary_go_identities_observed: u64,
    pub transport_identities_observed: u64,
    pub pet_identities_observed: u64,
    pub corpse_identities_observed: u64,
    pub dynamic_identities_observed: u64,
    pub go_add_counters_matched: u64,
    pub go_update_guids_matched: u64,
    pub go_destructors_observed: u64,
    pub independently_executed_constructors: u64,
    pub first_unavailable_constructor_event: Option<u64>,
    pub first_difference: Option<Seal8GameObjectDifference>,
    pub pending: Vec<&'static str>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
#[derive(Debug, Serialize)]
pub struct Seal8GameObjectDifference {
    pub event: u64,
    pub reason: Refusal,
    pub expected: GuidProjectionOperation,
    pub actual: GuidProjectionOperation,
}
struct Observed {
    generation: u64,
    guid: SourceGuid,
}
fn pointer(text: Option<&str>) -> Result<u64> {
    u64::from_str_radix(
        text.and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| refuse("execution.seal8_go_pointer"))?,
        16,
    )
    .map_err(|_| refuse("execution.seal8_go_pointer"))
}
fn compare(
    report: &mut Seal8GameObjectReport,
    event: u64,
    expected: GuidProjectionOperation,
    actual: GuidProjectionOperation,
) -> bool {
    if report.first_difference.is_some() {
        return false;
    }
    if let Err(error) = DifferentialOracle::compare_guid_projection(expected, actual) {
        report.first_difference = Some(Seal8GameObjectDifference {
            event,
            reason: error.reason,
            expected,
            actual,
        });
        false
    } else {
        true
    }
}
// _Create supplies only identity arguments, not independent GO constructor operands; Object.cpp:115-125, GameObject.cpp:178-302.
pub fn compare_seal8_unsealed_gameobjects(root: &Path) -> Result<Seal8GameObjectReport> {
    let mut report = Seal8GameObjectReport {
        adapter: "benilla-seal8-08-unsealed-go-constructor-observations-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records: 0,
        ordinary_go_identities_observed: 0,
        transport_identities_observed: 0,
        pet_identities_observed: 0,
        corpse_identities_observed: 0,
        dynamic_identities_observed: 0,
        go_add_counters_matched: 0,
        go_update_guids_matched: 0,
        go_destructors_observed: 0,
        independently_executed_constructors: 0,
        first_unavailable_constructor_event: None,
        first_difference: None,
        pending: vec![
            "sealed spawn/template/manager occurrence binding",
            "source constructor arguments and causal joins",
            "dynamic collision/model owner",
            "AI/linked/transport attachment owners",
            "native construction and registration generations",
        ],
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    };
    let mut objects = BTreeMap::<u64, Observed>::new();
    let mut generations = BTreeMap::<u64, SourceGuid>::new();
    report.records = scan_seal8(root, |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "object_identity" => {
                let pointer = pointer(fields.next())?;
                let _database: u32 = parse(fields.next())?;
                let low: u32 = parse(fields.next())?;
                let entry: u32 = parse(fields.next())?;
                let high: u32 = parse(fields.next())?;
                let generation: u64 = parse(fields.next())?;
                match high {
                    0xf110 => report.ordinary_go_identities_observed += 1,
                    0xf120 | 0x1fc0 => report.transport_identities_observed += 1,
                    0xf140 => report.pet_identities_observed += 1,
                    0xf101 => report.corpse_identities_observed += 1,
                    0xf100 => report.dynamic_identities_observed += 1,
                    _ => return Ok(()),
                }
                if high == 0xf110 || high == 0xf120 || high == 0x1fc0 {
                    require(objects.len() < 100_000, "execution.seal8_go_memory_bound")?;
                    let high = match high {
                        0xf110 => HighGuid::GameObject,
                        0xf120 => HighGuid::Transport,
                        _ => HighGuid::MoTransport,
                    };
                    let guid = SourceGuid::with_entry(high, entry, low);
                    require(
                        !objects.contains_key(&pointer),
                        "execution.seal8_go_identity_reassignment",
                    )?;
                    objects.insert(pointer, Observed { generation, guid });
                    generations.insert(generation, guid);
                    report
                        .first_unavailable_constructor_event
                        .get_or_insert(context.event);
                }
            }
            "object_add" => {
                let pointer = pointer(fields.next())?;
                let _map: u32 = parse(fields.next())?;
                let low: u32 = parse(fields.next())?;
                let generation: u64 = parse(fields.next())?;
                if let Some(object) = objects.get(&pointer) {
                    let expected = GuidProjectionOperation {
                        kind: GuidProjectionKind::AddCounter,
                        captured_generation: generation,
                        value: low.into(),
                    };
                    let actual = GuidProjectionOperation {
                        captured_generation: object.generation,
                        value: object.guid.counter().into(),
                        ..expected
                    };
                    if compare(&mut report, context.event, expected, actual) {
                        report.go_add_counters_matched += 1;
                    }
                }
            }
            "object_update" => {
                if let Some(&guid) = generations.get(&context.object_generation) {
                    let expected = GuidProjectionOperation {
                        kind: GuidProjectionKind::UpdateFullGuid,
                        captured_generation: context.object_generation,
                        value: context.object_guid,
                    };
                    let actual = GuidProjectionOperation {
                        value: guid.raw(),
                        ..expected
                    };
                    if compare(&mut report, context.event, expected, actual) {
                        report.go_update_guids_matched += 1;
                    }
                }
            }
            "object_destroy" => {
                if let Some(object) = objects.remove(&pointer(fields.next())?) {
                    generations.remove(&object.generation);
                    report.go_destructors_observed += 1;
                }
            }
            _ => {}
        }
        Ok(())
    })?;
    require(
        objects.is_empty() && generations.is_empty(),
        "execution.seal8_go_lifetime_exhaustion",
    )?;
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_seal8_gameobject_constructors_or_clean_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path = explicit
            .clone()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(2)
                    .unwrap()
                    .join(
                        "benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08",
                    )
            });
        if explicit.is_none() && !path.try_exists().unwrap() {
            eprintln!("SKIP seal8-08 unsealed GO constructors: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_gameobjects(&path).unwrap();
        println!("{}", serde_json::to_string(&report).unwrap());
        assert!(
            report.ordinary_go_identities_observed > 0 && report.transport_identities_observed > 0
        );
        assert_eq!(report.independently_executed_constructors, 0);
        assert!(report.first_unavailable_constructor_event.is_some());
        assert!(report.first_difference.is_none());
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
    }
    #[test]
    fn diagnostic_retains_first_difference_and_does_not_accept_later_records() {
        let mut report = Seal8GameObjectReport {
            adapter: "authored",
            trace_sha256: Sha256Id::of(&[]),
            records: 0,
            ordinary_go_identities_observed: 0,
            transport_identities_observed: 0,
            pet_identities_observed: 0,
            corpse_identities_observed: 0,
            dynamic_identities_observed: 0,
            go_add_counters_matched: 0,
            go_update_guids_matched: 0,
            go_destructors_observed: 0,
            independently_executed_constructors: 0,
            first_unavailable_constructor_event: None,
            first_difference: None,
            pending: vec![],
            reference_execution_sealed: false,
            whole_execution_accepted: false,
        };
        let expected = GuidProjectionOperation {
            kind: GuidProjectionKind::AddCounter,
            captured_generation: 1,
            value: 7,
        };
        let actual = GuidProjectionOperation {
            value: 8,
            ..expected
        };
        assert!(!compare(&mut report, 15, expected, actual));
        assert!(!compare(&mut report, 16, expected, expected));
        let difference = report.first_difference.unwrap();
        assert_eq!(difference.event, 15);
        assert_eq!(difference.expected.value, 7);
        assert_eq!(difference.actual.value, 8);
    }
}
