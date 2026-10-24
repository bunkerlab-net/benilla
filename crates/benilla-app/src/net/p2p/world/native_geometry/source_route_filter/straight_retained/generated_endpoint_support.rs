// SPDX-License-Identifier: GPL-3.0-or-later
//! Separate supplemental endpoint evidence. No original height calls are synthesized.
use super::*;
use benilla_native_nav::{SourceRetainedEndpointHeight, SourceWarmPolyPhase};

pub(super) fn validate(
    terminal: &LoadedSourceRetainedStraight,
    detail: &LoadedSourceRetainedEndpointSupport,
    generated: &GeneratedQueryProvenance,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<FlatBand, ResidentFilterError> {
    check_time(deadline, cancelled)?;
    detail.check_terminal(terminal).map_err(native_error)?;
    if terminal.phase() != SourceWarmPolyPhase::SamePolygon
        || terminal.points() != [terminal.from(), terminal.requested_end()]
        || !terminal.diagnostics().heights.is_empty()
        || terminal.ray().is_some()
        || detail.triangle_tests() > 4096
        || generated.tiles().is_empty()
        || generated.tiles().len() > 9
    {
        return Err(Failure::UnsupportedNavigation.into());
    }
    let band = matched_band(&detail.probes()[0], generated)?;
    validate_samples(terminal.points(), detail.probes(), band)?;
    for probe in detail.probes() {
        check_time(deadline, cancelled)?;
        if matched_band(probe, generated)? != band {
            return Err(Failure::UnsupportedNavigation.into());
        }
    }
    check_time(deadline, cancelled)?;
    Ok(band)
}

fn matched_band(
    probe: &SourceRetainedEndpointHeight,
    generated: &GeneratedQueryProvenance,
) -> Result<FlatBand, Failure> {
    let mut matches = generated.tiles().iter().filter(|tile| {
        tile.map_id() == generated.binding().map
            && tile.expected_native_tile() == probe.tile_coordinates
            && tile.bmin_bits() == probe.tile_bounds_min
            && tile.bmax_bits() == probe.tile_bounds_max
    });
    let tile = matches.next().ok_or(Failure::Binding)?;
    if matches.next().is_some() || tile.status() != GenerationStatus::Generated {
        return Err(Failure::Binding);
    }
    let band = one_cell_band(tile.bmin_bits()[1], tile.ch_bits())?;
    let config = tile.source_config();
    let agent = [
        config.walkable_height,
        config.walkable_radius,
        config.walkable_climb,
    ]
    .map(|v| (f32::from_bits(tile.ch_bits()) * v as f32).to_bits());
    if probe.tile_reference == 0 || probe.tile_agent_bits != agent {
        return Err(Failure::Binding);
    }
    Ok(band)
}

/// Pure predicate inputs are not certificates. The caller additionally holds an
/// opaque terminal, current query and matched generation receipt.
fn validate_samples(
    points: &[[u32; 3]],
    probes: &[SourceRetainedEndpointHeight; 2],
    band: FlatBand,
) -> Result<(), Failure> {
    if points.len() != 2 || points.iter().all(|point| point[2] == band.plane) {
        return Err(Failure::UnsupportedNavigation);
    }
    let h = f32::from_bits(band.plane);
    let q = f32::from_bits(band.ceiling);
    for (point, probe) in points.iter().zip(probes) {
        if *point != probe.point {
            return Err(Failure::Binding);
        }
        if point.iter().any(|v| !f32::from_bits(*v).is_finite())
            || !(h..=q).contains(&f32::from_bits(point[2]))
            || probe.polygon == 0
            || probe.polygon != probes[0].polygon
            || probe.tile_reference != probes[0].tile_reference
            || probe.polygon_index != probes[0].polygon_index
            || probe.query_status != 1 << 30
            || probe.query_height_bits != band.ceiling
            || probe.triangle_index >= probe.detail_triangle_count
            || probe.vertices.iter().enumerate().any(|(index, vertex)| {
                vertex.source != SourceDetailVertexSource::Polygon
                    || u32::from(probe.triangle[index]) >= probe.polygon_vertex_count
                    || vertex.detour_bits[1] != band.ceiling
                    || vertex
                        .detour_bits
                        .iter()
                        .any(|v| !f32::from_bits(*v).is_finite())
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
    fn observations() -> ([[u32; 3]; 2], [SourceRetainedEndpointHeight; 2], FlatBand) {
        let band = one_cell_band((-50_f32).to_bits(), BASE_UNIT_DIM.to_bits()).unwrap();
        let points = [
            [1_f32.to_bits(), 0, (-49.9_f32).to_bits()],
            [2_f32.to_bits(), 0, band.plane],
        ];
        let probes = points.map(|point| SourceRetainedEndpointHeight {
            point,
            polygon: 1,
            query_status: 1 << 30,
            query_height_bits: band.ceiling,
            tile_reference: 1,
            tile_coordinates: [0; 3],
            tile_bounds_min: [0, band.plane, 0],
            tile_bounds_max: [3_f32.to_bits(); 3],
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
        (points, probes, band)
    }
    #[test]
    fn supplemental_height_is_distinct_from_unchanged_endpoint_z() {
        let (points, probes, band) = observations();
        assert_ne!(probes[0].query_height_bits, points[0][2]);
        assert_ne!(probes[1].query_height_bits, points[1][2]);
        assert!(validate_samples(&points, &probes, band).is_ok());
    }
    #[test]
    fn endpoint_order_and_exact_band_are_required() {
        let (points, probes, band) = observations();
        assert_eq!(
            validate_samples(&points, &[probes[1], probes[0]], band),
            Err(Failure::Binding)
        );
        for z in [
            f32::from_bits(band.plane).to_bits() + 1,
            band.ceiling - 1,
            f32::NAN.to_bits(),
        ] {
            let mut points = points;
            let mut probes = probes;
            points[0][2] = z;
            probes[0].point = points[0];
            assert!(validate_samples(&points, &probes, band).is_err());
        }
        let points = points.map(|mut p| {
            p[2] = band.plane;
            p
        });
        let probes = std::array::from_fn(|i| {
            let mut p = probes[i];
            p.point = points[i];
            p
        });
        assert!(validate_samples(&points, &probes, band).is_err());
    }
    #[test]
    fn supplemental_success_polygon_and_triangle_must_match_exact_receipt_band() {
        let (points, probes, band) = observations();
        for kind in 0..8 {
            let mut probes = probes;
            match kind {
                0 => probes[1].query_status |= 1,
                1 => probes[1].query_height_bits = band.plane,
                2 => probes[1].polygon += 1,
                3 => probes[1].tile_reference += 1,
                4 => probes[1].polygon_index += 1,
                5 => probes[1].vertices[0].source = SourceDetailVertexSource::Detail,
                6 => probes[1].vertices[0].detour_bits[1] = band.ceiling + 1,
                7 => probes[1].triangle_index = probes[1].detail_triangle_count,
                _ => unreachable!(),
            }
            assert_eq!(
                validate_samples(&points, &probes, band),
                Err(Failure::UnsupportedNavigation)
            );
        }
    }
}
