// SPDX-License-Identifier: GPL-3.0-or-later
//! Conditional predicate projection. Captured geometry is never native geometry evidence.
use super::path_distance::{nearest_distance, reference_append};
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PathDistancePending {
    IndependentNativeP0P1P3Execution,
    IndependentNativeNearestGeometry,
    ReviewedPostDeviationOracleIdentity,
    SealedReferenceExecution,
}
#[derive(Debug, Serialize)]
pub struct StrictDistanceProjection {
    pub matching_predicates_before_first_difference: u64,
    pub first_recorded_predicate_difference: Option<DistanceBranchComparison>,
    pub matched_world_records: Option<u64>,
}
#[derive(Debug, Serialize)]
pub struct IntentDistanceProjection {
    pub recorded_nearest_predicates: u64,
    pub equal_predicates: u64,
    pub absent_distance_differences: u64,
    pub native_geometry_executed: bool,
    pub post_deviation_gameplay_accepted: bool,
}
#[derive(Debug, Serialize)]
pub struct Seal8DistanceReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub storage_reads_bound: u64,
    pub by_caller: BTreeMap<u8, u64>,
    pub strict: StrictDistanceProjection,
    pub intent: IntentDistanceProjection,
    pub pending: Vec<PathDistancePending>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
#[derive(Default)]
struct Nearest {
    operation: u64,
    point: Option<[u32; 3]>,
    extent: Option<[u32; 3]>,
    closest: Option<[u32; 3]>,
    filter: Option<(u16, u16)>,
    costs: Option<[u32; 64]>,
    cost_mask: u64,
    polygon: Option<u64>,
    status: Option<u32>,
}
fn vector(payload: &str) -> Result<[u32; 3]> {
    let mut fields = payload.split_whitespace();
    require(
        fields.next().is_some()
            && ["0", "4", "12", "0"]
                .into_iter()
                .all(|value| fields.next() == Some(value)),
        "execution.distance_vector_width",
    )?;
    let text = fields
        .next()
        .ok_or_else(|| refuse("execution.distance_vector_bytes"))?;
    require(
        fields.next().is_none() && text.len() == 24 && text.is_ascii(),
        "execution.distance_vector_bytes",
    )?;
    let mut result = [0; 3];
    for (value, part) in result.iter_mut().zip(text.as_bytes().as_chunks::<8>().0) {
        let mut raw = [0; 4];
        for (byte, hex) in raw.iter_mut().zip(part.as_chunks::<2>().0) {
            *byte = u8::from_str_radix(
                std::str::from_utf8(hex).map_err(|_| refuse("execution.distance_vector_bytes"))?,
                16,
            )
            .map_err(|_| refuse("execution.distance_vector_bytes"))?;
        }
        *value = u32::from_le_bytes(raw);
    }
    Ok(result)
}
// Join the storage read to its completed nearest operation and live logical birth; ReferenceChoices.h:328-350, ReferenceDetour.h:20-31.
pub fn compare_seal8_unsealed_distances(root: &Path) -> Result<Seal8DistanceReport> {
    let mut live = BTreeSet::new();
    let mut active = BTreeMap::<u64, Nearest>::new();
    let mut completed = BTreeMap::<u64, Nearest>::new();
    let mut occurrences = BTreeMap::<u64, u64>::new();
    let mut by_caller = BTreeMap::new();
    let mut strict = StrictDistanceProjection {
        matching_predicates_before_first_difference: 0,
        first_recorded_predicate_difference: None,
        matched_world_records: None,
    };
    let mut intent = IntentDistanceProjection {
        recorded_nearest_predicates: 0,
        equal_predicates: 0,
        absent_distance_differences: 0,
        native_geometry_executed: false,
        post_deviation_gameplay_accepted: false,
    };
    let records = scan_seal8(root, |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        if kind == "logical_birth" {
            let generation: u64 = parse(fields.next())?;
            if fields.next() == Some("object") {
                require(
                    live.insert(generation),
                    "execution.distance_duplicate_birth",
                )?;
            }
        } else if kind == "logical_death" {
            let generation: u64 = parse(fields.next())?;
            live.remove(&generation);
        } else if kind == "operation_begin" && payload.ends_with(" dtNavMeshQuery::findNearestPoly")
        {
            require(
                !active.contains_key(&context.thread),
                "execution.distance_nested_nearest",
            )?;
            active.insert(
                context.thread,
                Nearest {
                    operation: context.operation,
                    ..Default::default()
                },
            );
            completed.remove(&context.thread);
        } else if kind == "operation_end" && payload.ends_with(" dtNavMeshQuery::findNearestPoly") {
            let query = active
                .remove(&context.thread)
                .ok_or_else(|| refuse("execution.distance_nearest_end"))?;
            require(
                query.operation == context.operation,
                "execution.distance_nearest_operation",
            )?;
            completed.insert(context.thread, query);
        } else if kind == "choice_storage" {
            require(
                parse::<u64>(fields.next())? == 337362,
                "execution.distance_storage_family",
            )?;
            let caller = DistanceCaller::try_from(parse::<u8>(fields.next())?)
                .map_err(|_| refuse("execution.distance_caller"))?;
            let occurrence: u64 = parse(fields.next())?;
            let bits: u32 = parse(fields.next())?;
            let generation: u64 = parse(fields.next())?;
            require(
                fields.next().is_none() && live.contains(&generation),
                "execution.distance_live_generation",
            )?;
            let ordinal = occurrences.entry(context.thread).or_default();
            require(occurrence == *ordinal + 1, "execution.distance_occurrence")?;
            *ordinal = occurrence;
            let query = completed
                .remove(&context.thread)
                .ok_or_else(|| refuse("execution.distance_missing_nearest"))?;
            require(
                query.status.is_some_and(|s| s & 0x40000000 != 0)
                    && query.polygon.is_some_and(|p| p != 0)
                    && query.cost_mask == u64::MAX
                    && query.filter.is_some(),
                "execution.distance_nearest_result",
            )?;
            let point = query
                .point
                .ok_or_else(|| refuse("execution.distance_center"))?;
            let closest = query
                .closest
                .ok_or_else(|| refuse("execution.distance_closest"))?;
            let extent = query
                .extent
                .ok_or_else(|| refuse("execution.distance_extent"))?;
            require(
                extent == [5.0_f32.to_bits(); 3] || extent == [10.0_f32.to_bits(); 3],
                "execution.distance_extent",
            )?;
            let distance_bits = nearest_distance(point, closest)?.to_bits();
            let appended = reference_append(bits, distance_bits);
            intent.recorded_nearest_predicates += 1;
            *by_caller.entry(u8::from(caller)).or_default() += 1;
            if appended {
                intent.equal_predicates += 1;
                if strict.first_recorded_predicate_difference.is_none() {
                    strict.matching_predicates_before_first_difference += 1;
                }
            } else {
                intent.absent_distance_differences += 1;
                if strict.first_recorded_predicate_difference.is_none() {
                    strict.first_recorded_predicate_difference = Some(DistanceBranchComparison {
                        owner: DistanceReadOwner {
                            caller,
                            thread: context.thread,
                            object_generation: generation,
                            occurrence,
                        },
                        source_event: context.event,
                        reference_prior: ReferencePriorDistance::Unwritten {
                            recorded_bits: bits,
                        },
                        distance_bits,
                        reference_nearest: PathNearestObservation {
                            owner: DistanceReadOwner {
                                caller,
                                thread: context.thread,
                                object_generation: generation,
                                occurrence,
                            },
                            point_bits: point,
                            extent_bits: extent,
                            include_flags: query.filter.unwrap().0,
                            exclude_flags: query.filter.unwrap().1,
                            area_cost_bits: query.costs.unwrap().to_vec(),
                            status: query.status.unwrap(),
                            polygon: query.polygon.unwrap(),
                            closest_bits: closest,
                            distance_bits,
                        },
                        native_nearest: None,
                        reference_appended: false,
                        native_appended: true,
                        approved_absent_distance_difference: true,
                    });
                }
            }
        } else if let Some(query) = active.get_mut(&context.thread) {
            if context.operation != query.operation {
                return Ok(());
            }
            match kind {
                "detour_center" => {
                    query.point = Some(vector(payload)?);
                }
                "detour_extents" => {
                    query.extent = Some(vector(payload)?);
                }
                "detour_point_result" => {
                    query.closest = Some(vector(payload)?);
                }
                "detour_filter" => {
                    require(
                        parse::<u64>(fields.next())? == query.operation,
                        "execution.distance_nearest_operation",
                    )?;
                    query.filter = Some((parse(fields.next())?, parse(fields.next())?));
                }
                "detour_area_cost" => {
                    require(
                        parse::<u64>(fields.next())? == query.operation,
                        "execution.distance_nearest_operation",
                    )?;
                    let index: u8 = parse(fields.next())?;
                    let value = u32::from_str_radix(fields.next().unwrap_or(""), 16)
                        .map_err(|_| refuse("execution.distance_area_cost"))?;
                    require(index < 64, "execution.distance_area_cost")?;
                    let mask = 1_u64 << index;
                    require(query.cost_mask & mask == 0, "execution.distance_area_cost")?;
                    query.costs.get_or_insert([0; 64])[usize::from(index)] = value;
                    query.cost_mask |= mask;
                }
                "detour_nearest_ref" => {
                    require(
                        parse::<u64>(fields.next())? == query.operation,
                        "execution.distance_nearest_operation",
                    )?;
                    query.polygon = Some(parse(fields.next())?);
                }
                "geometry_result" => {
                    require(
                        parse::<u64>(fields.next())? == query.operation
                            && fields.next() == Some("integer")
                            && fields.next() == Some("4"),
                        "execution.distance_status_width",
                    )?;
                    query.status = Some(parse(fields.next())?);
                }
                _ => {}
            }
        }
        Ok(())
    })?;
    require(active.is_empty(), "execution.distance_unfinished_nearest")?;
    Ok(Seal8DistanceReport {
        adapter: "benilla-seal8-08-distance-predicate-projection-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records,
        storage_reads_bound: intent.recorded_nearest_predicates,
        by_caller,
        strict,
        intent,
        pending: vec![
            PathDistancePending::IndependentNativeP0P1P3Execution,
            PathDistancePending::IndependentNativeNearestGeometry,
            PathDistancePending::ReviewedPostDeviationOracleIdentity,
            PathDistancePending::SealedReferenceExecution,
        ],
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
