// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored loaded-query composition, not an original-source path oracle.
use super::*;
use crate::{
    LoadedSourceSamePolygon, SourceOwnerFilter, SourceRetainedStraightConfig,
    SourceSamePolygonConfig, TileInput,
};
use std::time::Duration;
const SLOT: crate::SourceMapQuerySlot = crate::SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const CONFIG: SourceRetainedStraightConfig = SourceRetainedStraightConfig {
    normalization: SourceSamePolygonConfig {
        normalize_z: false,
        ignore_normalization: false,
    },
    optimize_path: false,
};
fn query() -> SourceMapQuery {
    SourceMapQuery::load(
        SLOT,
        include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture"),
        &[TileInput {
            grid: [31, 31],
            bytes: include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
        }],
    )
    .unwrap()
}
fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn cold(selected: &SelectedSourceQuery<'_>) -> LoadedSourceSamePolygon {
    selected
        .loaded_cold_same_polygon_with_filter(
            1,
            SourceOwnerFilter::GROUND,
            CONFIG.normalization,
            Default::default(),
            until(),
        )
        .unwrap()
}
fn straight(
    selected: &SelectedSourceQuery<'_>,
    prior: &LoadedSourceSamePolygon,
) -> LoadedSourceRetainedStraight {
    selected
        .loaded_retained_straight_from_same_polygon(
            prior,
            CONFIG,
            0.125_f32.to_bits(),
            Default::default(),
            until(),
        )
        .unwrap()
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn short(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn bits(bytes: &[u8], at: usize) -> [u32; 3] {
    std::array::from_fn(|axis| word(bytes, at + axis * 4))
}

/// Independently decode the immutable serialized fixture. This checks which
/// actual vertex arrays were captured; it does not reconstruct witness authority.
fn assert_serialized_triangle(record: &SourceRetainedEndpointHeight, bytes: &[u8]) {
    let polygon_count = word(bytes, 44) as usize;
    let vertices = 120;
    let polygons = vertices + word(bytes, 48) as usize * 12;
    let details = polygons + polygon_count * 32 + word(bytes, 52) as usize * 16;
    let detail_vertices = details + word(bytes, 56) as usize * 12;
    let triangles = detail_vertices + word(bytes, 60) as usize * 12;
    let poly = polygons + record.polygon_index as usize * 32;
    let detail = details + record.polygon_index as usize * 12;
    assert_eq!(record.polygon_vertex_count, u32::from(bytes[poly + 30]));
    assert_eq!(record.detail_triangle_count, u32::from(bytes[detail + 9]));
    let triangle = triangles + (word(bytes, detail + 4) + record.triangle_index) as usize * 4;
    assert_eq!(record.triangle, bytes[triangle..triangle + 4]);
    for (i, vertex) in record.vertices.iter().enumerate() {
        let local = usize::from(record.triangle[i]);
        let (source, index, at) = if local < record.polygon_vertex_count as usize {
            let index = u32::from(short(bytes, poly + 4 + local * 2));
            (
                SourceDetailVertexSource::Polygon,
                index,
                vertices + index as usize * 12,
            )
        } else {
            let index = word(bytes, detail) + local as u32 - record.polygon_vertex_count;
            (
                SourceDetailVertexSource::Detail,
                index,
                detail_vertices + index as usize * 12,
            )
        };
        assert_eq!(vertex.source, source);
        assert_eq!(vertex.tile_index, index);
        assert_eq!(vertex.detour_bits, bits(bytes, at));
    }
    assert_eq!(
        record.tile_coordinates,
        [
            word(bytes, 28) as i32,
            word(bytes, 32) as i32,
            word(bytes, 36) as i32
        ]
    );
    assert_eq!(record.tile_bounds_min, bits(bytes, 92));
    assert_eq!(record.tile_bounds_max, bits(bytes, 104));
    assert_eq!(record.tile_agent_bits, bits(bytes, 80));
    assert_eq!(record.query_status, crate::SUCCESS);
}

#[test]
fn supplemental_heights_leave_native_endpoints_and_empty_original_calls_unchanged() {
    let source = query();
    let mut owner = source.retain_default();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.125), point(5., 6., 0.))
        .unwrap();
    let initial = cold(&selected);
    let terminal = straight(&selected, &initial);
    let before = format!("{terminal:?}");
    let witness = terminal
        .endpoint_support_witness(&selected, Default::default(), until())
        .unwrap();
    assert_eq!(witness.check_terminal(&terminal), Ok(()));
    assert_eq!(witness.check_query(&selected), Ok(()));
    assert_eq!(
        witness.probes().map(|p| p.point),
        [selected.from(), selected.to()]
    );
    assert_eq!(witness.probes()[0].query_height_bits, 0_f32.to_bits());
    assert_ne!(
        witness.probes()[0].query_height_bits,
        witness.probes()[0].point[2]
    );
    assert!(witness
        .probes()
        .iter()
        .all(|p| p.query_status == crate::SUCCESS && p.polygon == terminal.start_lookup().polygon));
    for probe in witness.probes() {
        assert_serialized_triangle(
            probe,
            include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
        );
    }
    assert_eq!(witness.triangle_tests(), 6); // diagonal first triangle, then second triangle.
    assert!(terminal.diagnostics().heights.is_empty());
    assert_eq!(
        terminal
            .detail_height_witness(
                &selected,
                SourceRetainedDetailBudget {
                    height_records: 0,
                    triangle_tests: 0
                },
                until()
            )
            .unwrap()
            .records()
            .len(),
        0
    );
    assert_eq!(format!("{terminal:?}"), before);
}

#[test]
fn two_probes_share_one_exact_triangle_budget_and_expiry() {
    let source = query();
    let mut owner = source.retain_default();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
        .unwrap();
    let initial = cold(&selected);
    let terminal = straight(&selected, &initial);
    let before = format!("{terminal:?}");
    for endpoints in [0, 1, 3, usize::MAX] {
        assert_eq!(
            terminal
                .endpoint_support_witness(
                    &selected,
                    SourceRetainedEndpointBudget {
                        endpoints,
                        triangle_tests: 4096
                    },
                    until()
                )
                .unwrap_err(),
            SourceRetainedDetailError::Boundary(Error::Budget)
        );
    }
    for triangle_tests in [0, 3, 5, 4097, usize::MAX] {
        assert_eq!(
            terminal
                .endpoint_support_witness(
                    &selected,
                    SourceRetainedEndpointBudget {
                        endpoints: 2,
                        triangle_tests
                    },
                    until()
                )
                .unwrap_err(),
            SourceRetainedDetailError::Boundary(Error::Budget)
        );
    }
    assert!(terminal
        .endpoint_support_witness(
            &selected,
            Default::default(),
            Instant::now() - Duration::from_millis(1)
        )
        .is_err());
    assert_eq!(
        terminal
            .endpoint_support_witness(
                &selected,
                SourceRetainedEndpointBudget {
                    endpoints: 2,
                    triangle_tests: 6
                },
                until()
            )
            .unwrap()
            .triangle_tests(),
        6
    );
    assert_eq!(format!("{terminal:?}"), before);
}

#[test]
fn equal_terminal_or_foreign_query_cannot_replace_actual_tokens() {
    let source = query();
    let mut owner = source.retain_default();
    let from = point(5., 5., 0.);
    let to = point(5., 6., 0.);
    let selected = owner.select_for_calculate(SLOT, from, to).unwrap();
    let initial = cold(&selected);
    let terminal = straight(&selected, &initial);
    let other = straight(&selected, &initial);
    assert_eq!(terminal.points(), other.points());
    let witness = terminal
        .endpoint_support_witness(&selected, Default::default(), until())
        .unwrap();
    assert_eq!(witness.check_terminal(&other), Err(Error::Identity));
    let foreign = query();
    let mut foreign_owner = foreign.retain_default();
    let foreign_selected = foreign_owner.select_for_calculate(SLOT, from, to).unwrap();
    assert_eq!(witness.check_query(&foreign_selected), Err(Error::Identity));
    assert_eq!(
        terminal
            .endpoint_support_witness(&foreign_selected, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Boundary(Error::Identity)
    );
    let changed = owner
        .select_for_calculate(SLOT, from, point(5., 7., 0.))
        .unwrap();
    assert_eq!(witness.check_query(&changed), Err(Error::Identity));
    assert_eq!(
        terminal
            .endpoint_support_witness(&changed, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Boundary(Error::Identity)
    );
}

#[test]
fn genuine_non_same_polygon_path_is_not_relabelled_endpoint_support() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
    );
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let terminal = straight(&selected, &initial);
    assert_ne!(terminal.phase(), crate::SourceWarmPolyPhase::SamePolygon);
    let before = format!("{terminal:?}");
    assert_eq!(
        terminal
            .endpoint_support_witness(&selected, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Boundary(Error::Unsupported)
    );
    assert_eq!(format!("{terminal:?}"), before);
}

#[test]
fn support_retains_only_query_and_current_identity_not_terminal_chain() {
    let source = query();
    let mut owner = source.retain_default();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
        .unwrap();
    let initial = Rc::new(cold(&selected));
    let prior = Rc::downgrade(&initial);
    let terminal = Rc::new(straight(&selected, &initial));
    let object = Rc::downgrade(&terminal);
    let identity = Rc::downgrade(&terminal.identity);
    let witness = terminal
        .endpoint_support_witness(&selected, Default::default(), until())
        .unwrap();
    drop(initial);
    drop(terminal);
    drop(owner);
    drop(source);
    assert!(prior.upgrade().is_none());
    assert!(object.upgrade().is_none());
    assert!(identity.upgrade().is_some());
    let mut retained = witness.query.retain_default();
    let selected = retained
        .select_for_calculate(SLOT, witness.from, witness.to)
        .unwrap();
    assert_eq!(witness.check_query(&selected), Ok(()));
    drop(witness);
    assert!(identity.upgrade().is_none());
}

#[test]
fn nearest_same_polygon_does_not_authorize_an_outside_endpoint() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
    );
    let selected = owner
        .select_for_calculate(SLOT, point(-0.5, 5., 0.), point(-0.5, 6., 0.))
        .unwrap();
    let terminal = straight(&selected, &initial);
    assert_eq!(terminal.phase(), crate::SourceWarmPolyPhase::SamePolygon);
    assert!(terminal.diagnostics().heights.is_empty());
    let snapshot = format!("{terminal:?}");
    assert_eq!(
        terminal
            .endpoint_support_witness(&selected, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Unsupported {
            record: 0,
            reason: SourceRetainedDetailUnsupported::OutsidePolygon
        }
    );
    assert_eq!(format!("{terminal:?}"), snapshot);
}
