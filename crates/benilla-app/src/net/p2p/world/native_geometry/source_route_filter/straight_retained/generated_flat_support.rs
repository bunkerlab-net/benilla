// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only one-cell-expression Polygon-vertex family, not arbitrary detail support.
//! Raw continuous footprint, dry ground and complete gap/body clearance remain required.
use super::super::super::super::super::products::nav_generation::{
    GeneratedQueryProvenance, GeneratedTileProvenance,
};
use super::*;
use benilla_native_nav::{
    LoadedSourceRetainedStraightDetail, SourceDetailVertexSource, SourceRetainedDetailBudget,
    SourceRetainedDetailHeight,
};
use benilla_native_nav_generator::{GenerationStatus, BASE_UNIT_DIM};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlatBand {
    plane: u32,
    ceiling: u32,
}

/// Only the live provider may construct this capability. It retains the actual
/// native terminal/query tokens, not copied observations or a previous owner chain.
/// Keep it inside synchronous certification, never in a staged or committed reply.
pub(crate) struct ResidentGeneratedFlatSupport {
    detail: LoadedSourceRetainedStraightDetail,
    candidate: Rc<()>,
    expires: Instant,
    band: FlatBand,
}
impl ResidentGeneratedFlatSupport {
    pub(crate) fn plane_bits(&self) -> u32 {
        self.band.plane
    }
    pub(crate) fn ceiling_bits(&self) -> u32 {
        self.band.ceiling
    }
}

impl ResidentFilterProvider<'_, '_, '_, '_> {
    /// Raised interior samples require genuine indexed native attribution. Exact
    /// runtime/native endpoints are only bounded, never claimed to be height calls.
    /// The separate all-H strict-plane arm must not invoke this constructor.
    pub(crate) fn observe_generated_flat_support(
        &mut self,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentGeneratedFlatSupport, ResidentFilterError> {
        self.require_generated_flat_support()?;
        let deadline = dependency.deadline(deadline);
        self.straight_for(dependency, origin, context, deadline, &cancelled)?;
        let detail = {
            let mut constructor = self
                .query
                .retain_constructor(context)
                .map_err(ResidentFilterError::Query)?;
            let selected = constructor
                .select_for_calculate(deadline)
                .map_err(ResidentFilterError::Query)?;
            let detail = dependency
                .terminal
                .detail_height_witness(
                    &selected,
                    SourceRetainedDetailBudget {
                        height_records: STRAIGHT_MAX_POINTS - 2,
                        triangle_tests: 4096,
                    },
                    deadline,
                )
                .map_err(ResidentFilterError::StraightDetail)?;
            detail
                .check_terminal(&dependency.terminal)
                .map_err(native_error)?;
            detail.check_query(&selected).map_err(native_error)?;
            detail
        };
        let generated = self.generated_flat_receipt()?;
        let band = validate_generated_band(
            &dependency.terminal,
            &detail,
            generated,
            deadline,
            &cancelled,
        )?;
        let support = ResidentGeneratedFlatSupport {
            detail,
            candidate: Rc::clone(&dependency.root.identity),
            expires: deadline,
            band,
        };
        self.check_generated_flat_support(
            &support, dependency, origin, context, deadline, cancelled,
        )?;
        Ok(support)
    }

    /// Revalidate before using bounded metadata and after concrete certification.
    /// A fresh caller deadline cannot renew the original attempt or live receipt.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_generated_flat_support(
        &mut self,
        support: &ResidentGeneratedFlatSupport,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        self.require_generated_flat_support()?;
        let deadline = dependency.deadline(deadline).min(support.expires);
        self.straight_for(dependency, origin, context, deadline, &cancelled)?;
        if !Rc::ptr_eq(&support.candidate, &dependency.root.identity) {
            return Err(Failure::Binding.into());
        }
        support
            .detail
            .check_terminal(&dependency.terminal)
            .map_err(native_error)?;
        {
            let mut constructor = self
                .query
                .retain_constructor(context)
                .map_err(ResidentFilterError::Query)?;
            let selected = constructor
                .select_for_calculate(deadline)
                .map_err(ResidentFilterError::Query)?;
            support
                .detail
                .check_query(&selected)
                .map_err(native_error)?;
        }
        let generated = self.generated_flat_receipt()?;
        if validate_generated_band(
            &dependency.terminal,
            &support.detail,
            generated,
            deadline,
            &cancelled,
        )? != support.band
        {
            return Err(Failure::Binding.into());
        }
        self.straight_for(dependency, origin, context, deadline, cancelled)?;
        Ok(())
    }

    /// Both dependencies must come from genuine separate source attempts. This
    /// probes root/terminal and live-dispatch binding, not a foreign query load.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assert_generated_flat_support_bindings_for_test(
        &mut self,
        dependency: &ResidentJoinedStraightDependency,
        other_attempt: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        let constructors = self.retained_constructor_count();
        let support =
            self.observe_generated_flat_support(dependency, origin, context, deadline, &cancelled)?;
        let encoded = serde_json::to_vec(context).map_err(|_| Failure::Invalid)?;
        let decoded: AuthorityRequest =
            serde_json::from_slice(&encoded).map_err(|_| Failure::Invalid)?;
        assert_eq!(encoded, serde_json::to_vec(&decoded).unwrap());
        assert_eq!(*context, decoded);
        assert!(!same_dispatch(context, &decoded));
        assert!(matches!(
            self.check_generated_flat_support(
                &support, dependency, origin, &decoded, deadline, &cancelled,
            ),
            Err(ResidentFilterError::Context(Failure::Binding))
        ));
        assert!(!Rc::ptr_eq(
            &dependency.root.identity,
            &other_attempt.root.identity,
        ));
        // Validate the distinct genuine attempt before attributing its refusal
        // to the support capability's candidate-root identity check.
        self.straight_for(other_attempt, origin, context, deadline, &cancelled)?;
        assert!(matches!(
            self.check_generated_flat_support(
                &support,
                other_attempt,
                origin,
                context,
                deadline,
                &cancelled,
            ),
            Err(ResidentFilterError::Context(Failure::Binding))
        ));
        assert!(matches!(
            support.detail.check_terminal(&other_attempt.terminal),
            Err(benilla_native_nav::Error::Identity)
        ));
        self.check_generated_flat_support(
            &support, dependency, origin, context, deadline, cancelled,
        )?;
        assert_eq!(self.retained_constructor_count(), constructors);
        eprintln!(
            "resident generated flat support bindings: source-revision={} decoded-live-seal=Binding distinct-attempt-root=Binding distinct-native-terminal=Identity original-recheck=true constructor-count-unchanged=true no-facts=true",
            dependency.source_revision(),
        );
        Ok(())
    }

    fn generated_flat_receipt(&self) -> Result<&GeneratedQueryProvenance, ResidentFilterError> {
        // This accessor checks the loader receipt's retained allocation against
        // this provider's query; equal bytes or descriptors cannot replace it.
        let generated = self
            .query
            .generated_provenance_for_test()
            .map_err(ResidentFilterError::Query)?
            .ok_or(Failure::MissingGeometry)?;
        if generated.binding() != self.query.binding()
            || generated.configuration_binding() != self.query.configuration_binding()
            || generated.binding().map != self.height.binding().map
            || generated.binding().source_epoch != self.height.generated_query_source_epoch()
        {
            return Err(Failure::Binding.into());
        }
        Ok(generated)
    }
}

fn validate_generated_band(
    terminal: &LoadedSourceRetainedStraight,
    detail: &LoadedSourceRetainedStraightDetail,
    generated: &GeneratedQueryProvenance,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<FlatBand, ResidentFilterError> {
    check_time(deadline, cancelled)?;
    let first = detail
        .records()
        .first()
        .ok_or(Failure::UnsupportedNavigation)?;
    if generated.tiles().is_empty() || generated.tiles().len() > 9 {
        return Err(Failure::Binding.into());
    }
    let band = matched_band(first, generated)?;
    validate_indexed_samples(
        terminal.points(),
        &terminal.diagnostics().heights,
        detail.records(),
        band,
    )?;
    for record in detail.records() {
        check_time(deadline, cancelled)?;
        if matched_band(record, generated)? != band {
            return Err(Failure::UnsupportedNavigation.into());
        }
    }
    check_time(deadline, cancelled)?;
    Ok(band)
}

fn matched_band(
    record: &SourceRetainedDetailHeight,
    generated: &GeneratedQueryProvenance,
) -> Result<FlatBand, Failure> {
    let mut matches = generated.tiles().iter().filter(|tile| {
        tile.map_id() == generated.binding().map
            && tile.expected_native_tile() == record.tile_coordinates
            && tile.bmin_bits() == record.tile_bounds_min
            && tile.bmax_bits() == record.tile_bounds_max
    });
    let tile = matches.next().ok_or(Failure::Binding)?;
    if matches.next().is_some() {
        return Err(Failure::Binding);
    }
    tile_band(tile, record)
}

fn tile_band(
    tile: &GeneratedTileProvenance,
    record: &SourceRetainedDetailHeight,
) -> Result<FlatBand, Failure> {
    if tile.status() != GenerationStatus::Generated {
        return Err(Failure::Binding);
    }
    let band = one_cell_band(tile.bmin_bits()[1], tile.ch_bits())?;
    let config = tile.source_config();
    let expected_agent = [
        config.walkable_height,
        config.walkable_radius,
        config.walkable_climb,
    ]
    .map(|value| (f32::from_bits(tile.ch_bits()) * value as f32).to_bits());
    if record.tile_reference == 0 || record.tile_agent_bits != expected_agent {
        return Err(Failure::Binding);
    }
    Ok(band)
}

fn one_cell_band(plane: u32, ch: u32) -> Result<FlatBand, Failure> {
    let h = f32::from_bits(plane);
    if !h.is_finite() || ch != BASE_UNIT_DIM.to_bits() {
        return Err(Failure::UnsupportedNavigation);
    }
    // generator.cpp fixes ch=BASE_UNIT_DIM; pinned DetourNavMeshBuilder.cpp
    // stores Polygon Y as bminY + iv[1]*ch. Require the exact iv=1 expression's
    // output bits, without claiming to recover the original integer raster index.
    let q = h + 1.0_f32 * f32::from_bits(ch);
    if !q.is_finite() || q <= h {
        return Err(Failure::UnsupportedNavigation);
    }
    Ok(FlatBand {
        plane,
        ceiling: q.to_bits(),
    })
}

/// Predicate tests may supply observations here, but cannot mint the sealed capability.
fn validate_indexed_samples(
    points: &[[u32; 3]],
    recorded: &[SourceStraightHeight],
    attributed: &[SourceRetainedDetailHeight],
    band: FlatBand,
) -> Result<(), Failure> {
    if !(3..=STRAIGHT_MAX_POINTS).contains(&points.len())
        || recorded.len() != points.len() - 2
        || attributed.len() != recorded.len()
    {
        return Err(Failure::UnsupportedNavigation);
    }
    let h = f32::from_bits(band.plane);
    let q = f32::from_bits(band.ceiling);
    if points.iter().any(|point| {
        point
            .iter()
            .any(|value| !f32::from_bits(*value).is_finite())
            || !(h..=q).contains(&f32::from_bits(point[2]))
    }) || points.iter().all(|point| point[2] == band.plane)
    {
        return Err(Failure::UnsupportedNavigation);
    }
    // The unclipped producer emits one endpoint=4 height call for each resolved
    // interior lookup, in sample order. Requiring every lookup to resolve makes
    // record i correspond exactly to point i+1, including repeated equal values.
    for (index, (record, detail)) in recorded.iter().zip(attributed).enumerate() {
        if detail.recorded != *record {
            return Err(Failure::Binding);
        }
        if record.endpoint != 4
            || record.polygon == 0
            || record.before[..2] != record.after[..2]
            || record.after != points[index + 1]
            || record.status != 1 << 30
            || detail.query_status != 1 << 30
            || detail.query_height_bits != record.after[2]
            || detail.triangle_index >= detail.detail_triangle_count
            || detail.vertices.iter().enumerate().any(|(vertex, value)| {
                value.source != SourceDetailVertexSource::Polygon
                    || u32::from(detail.triangle[vertex]) >= detail.polygon_vertex_count
                    || value.detour_bits[1] != band.ceiling
                    || value
                        .detour_bits
                        .iter()
                        .any(|bits| !f32::from_bits(*bits).is_finite())
            })
        {
            return Err(Failure::UnsupportedNavigation);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_native_nav::SourceDetailVertex;

    // Authored predicate operands, never a generated receipt or loaded native witness.
    fn observations() -> (
        [[u32; 3]; 4],
        [SourceStraightHeight; 2],
        [SourceRetainedDetailHeight; 2],
        FlatBand,
    ) {
        let band = one_cell_band((-1.0_f32).to_bits(), BASE_UNIT_DIM.to_bits()).unwrap();
        let points = [
            [0.0_f32.to_bits(), 0, band.plane],
            [1.0_f32.to_bits(), 0, band.ceiling],
            [2.0_f32.to_bits(), 0, band.ceiling],
            [3.0_f32.to_bits(), 0, band.plane],
        ];
        let records = std::array::from_fn(|i| SourceStraightHeight {
            endpoint: 4,
            polygon: 1,
            before: [points[i + 1][0], 0, points[i][2]],
            after: points[i + 1],
            status: 1 << 30,
        });
        let attributed = records.map(|recorded| SourceRetainedDetailHeight {
            recorded,
            query_status: 1 << 30,
            query_height_bits: recorded.after[2],
            tile_reference: 1,
            tile_coordinates: [0; 3],
            tile_bounds_min: [0, band.plane, 0],
            tile_bounds_max: [4.0_f32.to_bits(); 3],
            tile_agent_bits: [0; 3],
            polygon_index: 0,
            polygon_vertex_count: 3,
            detail_triangle_count: 1,
            triangle_index: 0,
            triangle: [0, 1, 2, 0],
            vertices: std::array::from_fn(|i| SourceDetailVertex {
                source: SourceDetailVertexSource::Polygon,
                tile_index: i as u32,
                detour_bits: [(i as f32).to_bits(), band.ceiling, 0],
            }),
        });
        (points, records, attributed, band)
    }

    #[test]
    fn polygon_one_cell_expression_is_exact_not_a_height_tolerance() {
        let band = one_cell_band(3_259_498_496, 1_049_135_238).unwrap();
        assert_eq!(band.ceiling, 3_259_428_591);
        assert!(one_cell_band(f32::NAN.to_bits(), BASE_UNIT_DIM.to_bits()).is_err());
        assert!(one_cell_band(f32::MAX.to_bits(), BASE_UNIT_DIM.to_bits()).is_err());
        assert!(one_cell_band(0, (BASE_UNIT_DIM * 2.0).to_bits()).is_err());
    }

    #[test]
    fn bounded_native_endpoints_are_not_claimed_as_sampled_heights() {
        let (mut points, records, attributed, band) = observations();
        points[0][2] = band.ceiling;
        points[3][2] = band.ceiling;
        assert!(validate_indexed_samples(&points, &records, &attributed, band).is_ok());
        for height in [-1.000_001_f32, f32::from_bits(band.ceiling) + 0.000_001] {
            points[0][2] = height.to_bits();
            assert_eq!(
                validate_indexed_samples(&points, &records, &attributed, band),
                Err(Failure::UnsupportedNavigation)
            );
        }
    }

    #[test]
    fn indexed_interior_height_cannot_leave_the_exact_band() {
        let (points, records, attributed, band) = observations();
        for height in [-1.000_001_f32, f32::from_bits(band.ceiling) + 0.000_001] {
            let mut points = points;
            let mut records = records;
            let mut attributed = attributed;
            points[1][2] = height.to_bits();
            records[0].after = points[1];
            attributed[0].recorded = records[0];
            attributed[0].query_height_bits = points[1][2];
            assert_eq!(
                validate_indexed_samples(&points, &records, &attributed, band),
                Err(Failure::UnsupportedNavigation)
            );
        }
    }

    #[test]
    fn every_sample_is_indexed_not_found_by_numeric_membership() {
        let (points, records, attributed, band) = observations();
        assert!(validate_indexed_samples(&points, &records, &attributed, band).is_ok());
        let reordered_records = [records[1], records[0]];
        let reordered_attributed = [attributed[1], attributed[0]];
        assert!(
            validate_indexed_samples(&points, &reordered_records, &reordered_attributed, band)
                .is_err()
        );
        assert!(validate_indexed_samples(&points, &records[..1], &attributed[..1], band).is_err());
        assert!(validate_indexed_samples(&points[..2], &[], &[], band).is_err());
    }

    #[test]
    fn detail_vertices_wrong_planes_and_unsuccessful_attribution_refuse() {
        let (points, records, attributed, band) = observations();
        let mut changed = attributed;
        changed[0].vertices[0].source = SourceDetailVertexSource::Detail;
        assert!(validate_indexed_samples(&points, &records, &changed, band).is_err());
        changed = attributed;
        changed[0].vertices[0].detour_bits[1] = band.plane;
        assert!(validate_indexed_samples(&points, &records, &changed, band).is_err());
        changed = attributed;
        changed[0].query_status |= 1;
        assert!(validate_indexed_samples(&points, &records, &changed, band).is_err());
        changed = attributed;
        changed[0].query_height_bits = band.plane;
        assert!(validate_indexed_samples(&points, &records, &changed, band).is_err());
        let mut failed = records;
        failed[0].status = 1 << 31;
        changed = attributed;
        changed[0].recorded = failed[0];
        changed[0].query_status = failed[0].status;
        assert!(validate_indexed_samples(&points, &failed, &changed, band).is_err());
        changed = attributed;
        changed[0].recorded = records[1];
        assert_eq!(
            validate_indexed_samples(&points, &records, &changed, band),
            Err(Failure::Binding)
        );
    }

    #[test]
    fn all_plane_paths_are_not_minted_as_detail_supported() {
        let (mut points, mut records, mut attributed, band) = observations();
        for point in &mut points {
            point[2] = band.plane;
        }
        for (record, detail) in records.iter_mut().zip(&mut attributed) {
            record.after[2] = band.plane;
            record.status = 1 << 31;
            detail.recorded = *record;
        }
        assert!(validate_indexed_samples(&points, &records, &attributed, band).is_err());
        // This refusal says nothing about the unchanged all-H concrete strict arm,
        // which does not call this predicate or interpret unused height records.
    }
}
