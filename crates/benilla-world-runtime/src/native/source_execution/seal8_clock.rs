// SPDX-License-Identifier: GPL-3.0-or-later
//! Clock arithmetic and map-dispatch diagnostics, not executed map gameplay.
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use benilla_sim::source_time::WorldTimer;
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
pub struct Seal8ClockReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub world_ticks_matched: u64,
    pub map_begins_matched: u64,
    pub map_end_owners_matched: u64,
    pub source_map_generations: Vec<MapIdentity>,
    pub pending: Vec<ClockSchedulerPending>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
// Recompute measured diffs and the full map interval; Timer.h:50-60, MapManager.cpp:194-227.
pub fn compare_seal8_unsealed_clocks(root: &Path) -> Result<Seal8ClockReport> {
    let mut timer = WorldTimer::default();
    let mut maps = None::<SourceMapManager>;
    let mut map_generations = Vec::new();
    let mut world_ticks = 0;
    let mut map_begins = 0;
    let mut map_ends = 0;
    let mut pending = BTreeMap::<(u64, u32, u32), MapJob>::new();
    let mut running = BTreeMap::<u64, (u64, ClockPhaseOperation)>::new();
    let records = scan_seal8(root, |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "config" if fields.next() == Some("MapUpdateInterval") => {
                let _source = fields
                    .next()
                    .ok_or_else(|| refuse("execution.seal8_interval"))?;
                let interval: u32 = parse(fields.next())?;
                require(maps.is_none(), "execution.seal8_duplicate_interval")?;
                maps = Some(SourceMapManager::new(interval));
            }
            "logical_birth" => {
                let generation: u64 = parse(fields.next())?;
                if fields.next() == Some("map") {
                    require(map_generations.len() < 2, "execution.seal8_map_births")?;
                    // CreateContinents constructs map 0 then map 1; MapManager.cpp:96-106.
                    let map = MapIdentity {
                        map: map_generations.len() as u32,
                        instance: 0,
                        creation_generation: generation,
                    };
                    maps.as_mut()
                        .ok_or_else(|| refuse("execution.seal8_interval_missing"))?
                        .register(map)?;
                    map_generations.push(map);
                }
            }
            "world_tick" => {
                let previous: u32 = parse(fields.next())?;
                let current: u32 = parse(fields.next())?;
                let expected_diff: u32 = parse(fields.next())?;
                let actual_previous = timer.tick_time();
                let actual_diff = timer.tick(i64::from(current));
                DifferentialOracle::compare_clock_phase(
                    ClockPhaseOperation::WorldTick {
                        previous,
                        current,
                        diff: expected_diff,
                    },
                    ClockPhaseOperation::WorldTick {
                        previous: actual_previous,
                        current: timer.tick_time(),
                        diff: actual_diff,
                    },
                )
                .map_err(|error| error.reason)?;
                if world_ticks > 0 {
                    if let Some(jobs) = maps
                        .as_mut()
                        .ok_or_else(|| refuse("execution.seal8_interval_missing"))?
                        .begin(actual_diff)?
                    {
                        require(
                            !jobs.is_empty() && pending.is_empty(),
                            "execution.seal8_map_batch",
                        )?;
                        for job in jobs {
                            pending.insert((world_ticks, job.map.map, job.map.instance), job);
                        }
                    }
                }
                world_ticks += 1;
            }
            "map_begin" => {
                let map_id: u32 = parse(fields.next())?;
                let instance: u32 = parse(fields.next())?;
                let diff: u32 = parse(fields.next())?;
                let job = pending
                    .get(&(context.world, map_id, instance))
                    .ok_or_else(|| refuse("execution.seal8_unexpected_map_dispatch"))?;
                require(
                    context.map_update > 0 && !running.contains_key(&context.thread),
                    "execution.seal8_map_update_owner",
                )?;
                let expected = ClockPhaseOperation::MapBegin {
                    world: context.world,
                    map: MapIdentity {
                        map: context.map,
                        instance: context.instance,
                        creation_generation: context.map_generation,
                    },
                    diff,
                };
                let actual = ClockPhaseOperation::MapBegin {
                    world: world_ticks - 1,
                    map: job.map,
                    diff: job.diff,
                };
                require(
                    (map_id, instance) == (context.map, context.instance),
                    "execution.seal8_map_context",
                )?;
                DifferentialOracle::compare_clock_phase(expected, actual)
                    .map_err(|error| error.reason)?;
                running.insert(context.thread, (context.map_update, actual));
                map_begins += 1;
            }
            "map_end" => {
                let map_id: u32 = parse(fields.next())?;
                let instance: u32 = parse(fields.next())?;
                let (update, begin) = running
                    .remove(&context.thread)
                    .ok_or_else(|| refuse("execution.seal8_map_end_without_begin"))?;
                require(
                    update == context.map_update
                        && (map_id, instance) == (context.map, context.instance),
                    "execution.seal8_map_end_context",
                )?;
                let ClockPhaseOperation::MapBegin { world, map, .. } = begin else {
                    return Err(refuse("execution.seal8_map_begin_kind"));
                };
                let expected = ClockPhaseOperation::MapEndOwner {
                    world: context.world,
                    map: MapIdentity {
                        map: context.map,
                        instance: context.instance,
                        creation_generation: context.map_generation,
                    },
                };
                let actual = ClockPhaseOperation::MapEndOwner { world, map };
                DifferentialOracle::compare_clock_phase(expected, actual)
                    .map_err(|error| error.reason)?;
                require(
                    pending.remove(&(world, map.map, map.instance)).is_some(),
                    "execution.seal8_map_end_pending",
                )?;
                if pending.is_empty() {
                    maps.as_mut().unwrap().finish()?;
                }
                map_ends += 1;
            }
            _ => {}
        }
        Ok(())
    })?;
    require(
        world_ticks == 464
            && map_begins == 468
            && map_ends == 468
            && pending.is_empty()
            && running.is_empty(),
        "execution.seal8_clock_exhaustion",
    )?;
    Ok(Seal8ClockReport {
        adapter: "benilla-seal8-08-unsealed-clock-map-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records,
        world_ticks_matched: world_ticks,
        map_begins_matched: map_begins,
        map_end_owners_matched: map_ends,
        source_map_generations: map_generations,
        pending: CLOCK_SCHEDULER_PENDING.into(),
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_comparator_refuses_diff_and_owner_changes() {
        let tick = ClockPhaseOperation::WorldTick {
            previous: 100,
            current: 99,
            diff: 1,
        };
        let wrong = ClockPhaseOperation::WorldTick {
            previous: 100,
            current: 99,
            diff: u32::MAX,
        };
        assert!(DifferentialOracle::compare_clock_phase(tick, wrong).is_err());
        let map = MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 7,
        };
        assert!(DifferentialOracle::compare_clock_phase(
            ClockPhaseOperation::MapEndOwner { world: 3, map },
            ClockPhaseOperation::MapEndOwner { world: 4, map }
        )
        .is_err());
    }
    #[test]
    fn retained_seal8_clock_map_projection_or_clean_absence() {
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
            eprintln!("SKIP seal8-08 clock/map projection: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_clocks(&path).unwrap();
        assert_eq!(
            (
                report.world_ticks_matched,
                report.map_begins_matched,
                report.map_end_owners_matched
            ),
            (464, 468, 468)
        );
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
