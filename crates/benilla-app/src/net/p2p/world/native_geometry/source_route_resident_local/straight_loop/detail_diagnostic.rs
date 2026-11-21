// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded attribution on the original refusal path, never a support converter.
use super::super::super::{
    core::Body, products::source_height::CompositionError,
    source_route_filter::resident::retained::straight_retained::ResidentJoinedStraightDependency,
};
use super::*;
use std::fmt::Write;

#[allow(clippy::too_many_arguments)]
pub(super) fn inspect(
    terrain: &NativeTerrainCoverage,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    installed_owner: &benilla_formats::NativeInventorySource,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    dependency: &ResidentJoinedStraightDependency,
    origin: &ResidentColdCandidate,
    context: &AuthorityRequest,
    points: &[[u32; 3]],
    body: Body,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<String, ResidentFilterError> {
    let deadline = dependency.deadline(deadline);
    provider.verify_installed_owner(compatibility, installed_owner, deadline, cancelled)?;
    let map = compatibility.source_binding().0.map;
    // All strings stay private until fresh source AND installed-owner checks
    // succeed again. A failed diagnostic never substitutes a geometry result.
    let report = provider.inspect_straight_detail_for_test(
        dependency,
        &origin.dependency,
        context,
        deadline,
        cancelled,
        |witness, generated| {
            if points.len() > STRAIGHT_MAX_POINTS
                || witness.records().len() > STRAIGHT_MAX_POINTS - 1
                || generated.tiles().len() > 9
            {
                return Err(Failure::Invalid.into());
            }
            let mut report = String::new();
            let mut planes = Vec::new();
            planes.try_reserve_exact(generated.tiles().len()).map_err(|_| Failure::QueueCapacity)?;
            writeln!(report,
                "resident straight detail observation: tick={} source-revision={} records={} triangle-tests={} compatibility={} generated-binding={:?} configuration={:?} terrain-epoch={} collision-binding={:?}",
                context.binding.tick, dependency.source_revision(), witness.records().len(), witness.triangle_tests(), witness.source_revision(), generated.binding(), generated.configuration_binding(), generated.terrain_source_epoch(), generated.collision_source_binding(),
            ).map_err(|_| Failure::Invalid)?;
            for (index, record) in witness.records().iter().enumerate() {
                poll(deadline, cancelled)?;
                // Match the actual queried native tile, not a tile inferred from
                // route endpoints. Both witnesses retain the exact same query.
                let mut matches = generated.tiles().iter().filter(|tile| {
                    tile.map_id() == map
                        && tile.expected_native_tile() == record.tile_coordinates
                        && tile.bmin_bits() == record.tile_bounds_min
                        && tile.bmax_bits() == record.tile_bounds_max
                });
                let tile = matches.next().ok_or(Failure::Binding)?;
                if matches.next().is_some() {
                    return Err(Failure::Binding.into());
                }
                let ch = f32::from_bits(tile.ch_bits());
                let config = tile.source_config();
                let expected_agent = [config.walkable_height, config.walkable_radius, config.walkable_climb]
                    .map(|value| (ch * value as f32).to_bits());
                if record.tile_agent_bits != expected_agent {
                    return Err(Failure::Binding.into());
                }
                writeln!(report,
                    "resident straight detail record={index}: native={record:?}; generation source-grid={:?} request-tile={:?} native-tile={:?} map={:?} config={config:?} ch-bits={} status={:?} input={:?} output={:?} request-sha={:?} response-sha={:?} mmap-sha={:?} mmtile-sha={:?}",
                    tile.source_grid(), tile.request_tile(), tile.expected_native_tile(), tile.map_params(), tile.ch_bits(), tile.status(), tile.input_revision(), tile.output_revision(), tile.request_sha256(), tile.response_sha256(), tile.mmap_sha256(), tile.mmtile_sha256(),
                ).map_err(|_| Failure::Invalid)?;
                // bminY is ONLY a proposed projection plane until the existing
                // exact raw planar rectangle predicate succeeds below.
                let plane = tile.bmin_bits()[1];
                if !planes.contains(&plane) {
                    planes.push(plane);
                }
            }
            if witness.records().is_empty() {
                writeln!(report, "resident straight detail: no recorded height calls; no detail or raw-plane evidence")
                    .map_err(|_| Failure::Invalid)?;
            }
            for plane in planes {
                for (index, pair) in points.windows(2).enumerate() {
                    poll(deadline, cancelled)?;
                    // Proof-only projected operands. The native points, emitted
                    // motion, terminal and original refusal are never changed.
                    let from = [f32::from_bits(pair[0][0]), f32::from_bits(pair[0][1]), f32::from_bits(plane)];
                    let to = [f32::from_bits(pair[1][0]), f32::from_bits(pair[1][1]), f32::from_bits(plane)];
                    match terrain.prove(map, from, to, body.radius.as_f32()) {
                        Ok(coverage) => {
                            if coverage.map != map
                                || coverage.inventory_digest != installed_owner.inventory_digest()
                                || core::geometry_revision(coverage.map, coverage.wdt_source_digest, &coverage.tiles, &coverage.models, coverage.inventory_digest) != origin.raw_revision
                            {
                                return Err(Failure::Binding.into());
                            }
                            writeln!(report, "resident straight raw projected rectangle: plane-bits={plane} segment={index}/{} exact-planar-proof=true inventory={:?} wdt={:?} tiles={:?} terrain-root={:?}; NOT native-segment support or clearance", points.len() - 1, coverage.inventory_digest, coverage.wdt_source_digest, coverage.tiles, coverage.terrain_root)
                                .map_err(|_| Failure::Invalid)?;
                        }
                        Err(error) => writeln!(report, "resident straight raw projected rectangle: plane-bits={plane} segment={index}/{} error={error:?}; no raw-plane proof", points.len() - 1)
                            .map_err(|_| Failure::Invalid)?,
                    }
                }
            }
            Ok(report)
        },
    )?;
    provider.verify_installed_owner(compatibility, installed_owner, deadline, cancelled)?;
    Ok(report)
}

fn poll(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), ResidentFilterError> {
    if cancelled() {
        return Err(ResidentFilterError::Source(CompositionError::Cancelled));
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline.into());
    }
    Ok(())
}
