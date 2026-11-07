// SPDX-License-Identifier: GPL-3.0-or-later
//! Constructor observations cannot supply missing loader/manager input occurrences.
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
pub struct CreatureConstructorDifference {
    pub first_observed_identity_event: u64,
    pub observed_construct_event: u64,
    pub thread: u64,
    pub reference_construction_generation: u64,
    pub database_guid: u32,
    pub source_entry: u32,
    pub source_counter: u32,
    pub owner: &'static str,
    pub refusal: Refusal,
}
#[derive(Debug, Serialize)]
pub struct Seal8CreatureReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub generic_constructions_observed: u64,
    pub creature_identity_calls_observed: u64,
    pub database_creature_identity_calls_observed: u64,
    pub non_database_creature_identity_calls_observed: u64,
    pub complete_constructor_records_matched: u64,
    pub first_difference: Option<CreatureConstructorDifference>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
fn pointer(field: Option<&str>) -> Result<u64> {
    u64::from_str_radix(
        field
            .and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| refuse("execution.seal8_creature_pointer"))?,
        16,
    )
    .map_err(|_| refuse("execution.seal8_creature_pointer"))
}
// _Create identifies the creature only after LoadFromDB has already consumed inputs; Object.cpp:115-125, Creature.cpp:1602-1610.
pub fn compare_seal8_unsealed_creatures(root: &Path) -> Result<Seal8CreatureReport> {
    let mut constructed = BTreeMap::<u64, (u64, u64)>::new();
    let (mut generic, mut identities, mut database, mut non_database) = (0, 0, 0, 0);
    let mut first = None;
    let records = scan_seal8(root, |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "object_construct" => {
                let pointer = pointer(fields.next())?;
                require(
                    constructed.len() < 100_000 && !constructed.contains_key(&pointer),
                    "execution.seal8_creature_lifetime",
                )?;
                constructed.insert(pointer, (context.event, context.thread));
                generic += 1;
            }
            "object_identity" => {
                let pointer = pointer(fields.next())?;
                let guid: u32 = parse(fields.next())?;
                let counter: u32 = parse(fields.next())?;
                let entry: u32 = parse(fields.next())?;
                let high: u32 = parse(fields.next())?;
                let generation: u64 = parse(fields.next())?;
                if high == HighGuid::Creature as u32 {
                    let &(construct_event, construct_thread) = constructed
                        .get(&pointer)
                        .ok_or_else(|| refuse("execution.seal8_creature_without_constructor"))?;
                    require(
                        generation > 0 && construct_thread == context.thread,
                        "execution.seal8_creature_constructor_thread",
                    )?;
                    identities += 1;
                    if guid != 0 {
                        database += 1;
                    } else {
                        non_database += 1;
                    }
                    if first.is_none() {
                        first = Some(CreatureConstructorDifference {
                            first_observed_identity_event: context.event,
                            observed_construct_event: construct_event,
                            thread: context.thread,
                            reference_construction_generation: generation,
                            database_guid: guid, source_entry: entry, source_counter: counter,
                            owner: "ObjectGridLoader.cpp:134-135 / Creature.cpp:1615",
                            refusal: refuse("execution.creature.loader_manager_and_definition_occurrences_unbound"),
                        });
                    }
                }
            }
            "object_destroy" => {
                require(
                    constructed.remove(&pointer(fields.next())?).is_some(),
                    "execution.seal8_creature_destroy_without_constructor",
                )?;
            }
            _ => {}
        }
        Ok(())
    })?;
    require(
        constructed.is_empty(),
        "execution.seal8_creature_lifetime_exhaustion",
    )?;
    Ok(Seal8CreatureReport {
        adapter: "benilla-seal8-08-unsealed-creature-constructor-refusal-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records,
        generic_constructions_observed: generic,
        creature_identity_calls_observed: identities,
        database_creature_identity_calls_observed: database,
        non_database_creature_identity_calls_observed: non_database,
        complete_constructor_records_matched: 0,
        first_difference: first,
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_seal8_creature_constructors_or_clean_absence() {
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
            eprintln!("SKIP unsealed seal8-08 creature constructors: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_creatures(&path).unwrap();
        assert!(report.creature_identity_calls_observed > 0);
        assert_eq!(report.complete_constructor_records_matched, 0);
        assert!(report.first_difference.is_some());
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
