// SPDX-License-Identifier: GPL-3.0-or-later
//! _Create argument fixtures recompute GUID bits, never world allocator inputs.
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
pub struct Seal8ObjectReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub constructors_observed: u64,
    pub destructors_observed: u64,
    pub identity_calls_observed: u64,
    pub add_counters_matched: u64,
    pub update_full_guids_matched: u64,
    pub pending: Vec<ObjectRegistrationPending>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
struct CapturedObject {
    generation: u64,
    guid: Option<SourceGuid>,
}
fn address(text: Option<&str>) -> Result<u64> {
    u64::from_str_radix(
        text.and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| refuse("execution.seal8_object_address"))?,
        16,
    )
    .map_err(|_| refuse("execution.seal8_object_address"))
}
fn high_guid(value: u32) -> Result<HighGuid> {
    Ok(match value {
        0 => HighGuid::Player,
        0x4000 => HighGuid::Item,
        0xf110 => HighGuid::GameObject,
        0xf120 => HighGuid::Transport,
        0xf130 => HighGuid::Creature,
        0xf140 => HighGuid::Pet,
        0xf100 => HighGuid::DynamicObject,
        0xf101 => HighGuid::Corpse,
        0x1fc0 => HighGuid::MoTransport,
        _ => return Err(refuse("execution.seal8_object_high_guid")),
    })
}
// _Create records arguments; updates observe the resulting raw GUID; Object.cpp:115-125, ObjectGuid.h:87,103-111.
pub fn compare_seal8_unsealed_objects(root: &Path) -> Result<Seal8ObjectReport> {
    let mut objects = BTreeMap::<u64, CapturedObject>::new();
    let mut guids = BTreeMap::<u64, SourceGuid>::new();
    let mut constructors = 0;
    let mut destructors = 0;
    let mut identities = 0;
    let mut add_counters = 0;
    let mut update_guids = 0;
    let records = scan_seal8(root, |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "logical_birth" => {
                let generation: u64 = parse(fields.next())?;
                if fields.next() == Some("object") {
                    let _creator: u64 = parse(fields.next())?;
                    let _ordinal: u64 = parse(fields.next())?;
                    let pointer = address(fields.next())?;
                    require(
                        objects.len() < 100_000 && !objects.contains_key(&pointer),
                        "execution.seal8_object_lifetime",
                    )?;
                    objects.insert(
                        pointer,
                        CapturedObject {
                            generation,
                            guid: None,
                        },
                    );
                }
            }
            "object_construct" => {
                let pointer = address(fields.next())?;
                require(
                    objects.contains_key(&pointer),
                    "execution.seal8_constructor_birth",
                )?;
                constructors += 1;
            }
            "object_identity" => {
                let pointer = address(fields.next())?;
                let _database: u32 = parse(fields.next())?;
                let low: u32 = parse(fields.next())?;
                let entry: u32 = parse(fields.next())?;
                let high = high_guid(parse(fields.next())?)?;
                let generation: u64 = parse(fields.next())?;
                let object = objects
                    .get_mut(&pointer)
                    .ok_or_else(|| refuse("execution.seal8_identity_without_birth"))?;
                require(
                    object.generation == generation,
                    "execution.seal8_identity_generation",
                )?;
                let guid = SourceGuid::with_entry(high, entry, low);
                object.guid = Some(guid);
                guids.insert(generation, guid);
                identities += 1;
            }
            "object_add" => {
                let pointer = address(fields.next())?;
                let _map: u32 = parse(fields.next())?;
                let expected: u32 = parse(fields.next())?;
                let generation: u64 = parse(fields.next())?;
                let object = objects
                    .get(&pointer)
                    .ok_or_else(|| refuse("execution.seal8_add_without_birth"))?;
                require(
                    object.generation == generation,
                    "execution.seal8_add_generation",
                )?;
                let guid = object
                    .guid
                    .ok_or_else(|| refuse("execution.seal8_add_without_create"))?;
                DifferentialOracle::compare_guid_projection(
                    GuidProjectionOperation {
                        kind: GuidProjectionKind::AddCounter,
                        captured_generation: generation,
                        value: u64::from(expected),
                    },
                    GuidProjectionOperation {
                        kind: GuidProjectionKind::AddCounter,
                        captured_generation: object.generation,
                        value: u64::from(guid.counter()),
                    },
                )
                .map_err(|error| error.reason)?;
                add_counters += 1;
            }
            "object_update" => {
                let guid = guids
                    .get(&context.object_generation)
                    .ok_or_else(|| refuse("execution.seal8_update_without_create"))?;
                require(
                    context.object_generation > 0 && context.map_generation > 0,
                    "execution.seal8_update_generation",
                )?;
                DifferentialOracle::compare_guid_projection(
                    GuidProjectionOperation {
                        kind: GuidProjectionKind::UpdateFullGuid,
                        captured_generation: context.object_generation,
                        value: context.object_guid,
                    },
                    GuidProjectionOperation {
                        kind: GuidProjectionKind::UpdateFullGuid,
                        captured_generation: context.object_generation,
                        value: guid.raw(),
                    },
                )
                .map_err(|error| error.reason)?;
                update_guids += 1;
            }
            "object_destroy" => {
                let pointer = address(fields.next())?;
                let object = objects
                    .remove(&pointer)
                    .ok_or_else(|| refuse("execution.seal8_destroy_without_birth"))?;
                guids.remove(&object.generation);
                destructors += 1;
            }
            _ => {}
        }
        Ok(())
    })?;
    require(
        constructors == 3847
            && destructors == 3847
            && identities == 3767
            && add_counters == 3733
            && update_guids == 210_725
            && objects.is_empty()
            && guids.is_empty(),
        "execution.seal8_object_exhaustion",
    )?;
    Ok(Seal8ObjectReport {
        adapter: "benilla-seal8-08-unsealed-guid-bits-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records,
        constructors_observed: constructors,
        destructors_observed: destructors,
        identity_calls_observed: identities,
        add_counters_matched: add_counters,
        update_full_guids_matched: update_guids,
        pending: vec![
            ObjectRegistrationPending::SealedAllocatorStarts,
            ObjectRegistrationPending::ConstructionCausality,
            ObjectRegistrationPending::RegistrationAndCellState,
            ObjectRegistrationPending::GenerationAssociations,
            ObjectRegistrationPending::CameraAndViewpointCallbacks,
        ],
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_seal8_guid_projection_or_clean_absence() {
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
            eprintln!("SKIP seal8-08 GUID projection: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_objects(&path).unwrap();
        assert_eq!(report.add_counters_matched, 3733);
        assert_eq!(report.update_full_guids_matched, 210_725);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("{}", serde_json::to_string(&report).unwrap());
    }
    #[test]
    fn guid_projection_comparator_preserves_the_first_raw_difference() {
        let expected = GuidProjectionOperation {
            kind: GuidProjectionKind::UpdateFullGuid,
            captured_generation: 1,
            value: 0xf130_0000_2f00_0007,
        };
        let actual = GuidProjectionOperation {
            value: expected.value ^ 1,
            ..expected
        };
        let error = DifferentialOracle::compare_guid_projection(expected, actual).unwrap_err();
        assert_eq!(error.expected, expected);
        assert_eq!(error.actual, actual);
    }
}
