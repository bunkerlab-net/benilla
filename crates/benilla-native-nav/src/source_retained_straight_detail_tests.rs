// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded attribution/composition tests, not a new original-source oracle.
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
const TILE: &[u8] = include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture");
fn query() -> SourceMapQuery {
    SourceMapQuery::load(
        SLOT,
        include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture"),
        &[TileInput {
            grid: [31, 31],
            bytes: TILE,
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
fn assert_serialized_triangle(record: &SourceRetainedDetailHeight, bytes: &[u8]) {
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
    assert_eq!(record.query_status, record.recorded.status);
    assert_eq!(record.query_height_bits, record.recorded.after[2]);
}

#[test]
fn genuine_recorded_calls_capture_first_triangle_and_exact_loaded_vertices() {
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
    let witness = terminal
        .detail_height_witness(&selected, Default::default(), until())
        .unwrap();
    assert_eq!(witness.check_query(&selected), Ok(()));
    assert_eq!(witness.check_terminal(&terminal), Ok(()));
    assert_eq!(witness.records().len(), 2);
    // These two rectangles have explicit diagonals in the immutable fixture:
    // world(5,9) selects its second triangle, world(5,13) the first.
    assert_eq!(
        witness
            .records()
            .iter()
            .map(|r| r.triangle_index)
            .collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(witness.triangle_tests(), 6);
    for (record, original) in witness
        .records()
        .iter()
        .zip(&terminal.diagnostics().heights)
    {
        assert_eq!(&record.recorded, original);
        assert_serialized_triangle(record, TILE);
        assert!(record
            .vertices
            .iter()
            .all(|v| v.source == SourceDetailVertexSource::Polygon));
    }
}

#[test]
fn shared_detail_diagonal_keeps_original_first_matching_triangle() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 1., 0.), point(5., 2., 0.))
            .unwrap(),
    );
    let selected = owner
        .select_for_calculate(SLOT, point(5., 1., 0.), point(5., 15., 0.))
        .unwrap();
    let terminal = straight(&selected, &initial);
    let witness = terminal
        .detail_height_witness(&selected, Default::default(), until())
        .unwrap();
    let first = witness.records().first().unwrap();
    assert_eq!(first.recorded.before, point(5., 5., 0.));
    assert_eq!(first.triangle_index, 0);
    assert_serialized_triangle(first, TILE);
}

#[test]
fn budgets_and_deadline_fail_without_changing_terminal_or_predecessor() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
    );
    let prior_storage = initial.compatibility().storage().to_vec();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let terminal = straight(&selected, &initial);
    let snapshot = format!("{terminal:?}");
    let exact = SourceRetainedDetailBudget {
        height_records: 2,
        triangle_tests: 6,
    };
    for budget in [
        SourceRetainedDetailBudget {
            height_records: 1,
            ..exact
        },
        SourceRetainedDetailBudget {
            triangle_tests: 5,
            ..exact
        },
        SourceRetainedDetailBudget {
            height_records: SOURCE_RETAINED_DETAIL_MAX_HEIGHT_RECORDS + 1,
            ..exact
        },
        SourceRetainedDetailBudget {
            triangle_tests: SOURCE_RETAINED_DETAIL_MAX_TRIANGLE_TESTS + 1,
            ..exact
        },
    ] {
        assert_eq!(
            terminal
                .detail_height_witness(&selected, budget, until())
                .unwrap_err(),
            SourceRetainedDetailError::Boundary(Error::Budget)
        );
        assert_eq!(format!("{terminal:?}"), snapshot);
        assert_eq!(initial.compatibility().storage(), prior_storage);
    }
    assert_eq!(
        terminal
            .detail_height_witness(&selected, exact, Instant::now() - Duration::from_millis(1))
            .unwrap_err(),
        SourceRetainedDetailError::Boundary(Error::Deadline)
    );
    let witness = terminal
        .detail_height_witness(&selected, exact, until())
        .unwrap();
    assert_eq!(witness.triangle_tests(), exact.triangle_tests);
    assert_eq!(format!("{terminal:?}"), snapshot);
    assert_eq!(initial.compatibility().storage(), prior_storage);
}

#[test]
fn allocation_endpoints_and_exact_terminal_token_are_independent_bindings() {
    let source = query();
    let other_source = query();
    let mut owner = source.retain_default();
    let mut other_owner = other_source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
    );
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let terminal = straight(&selected, &initial);
    let sibling = straight(&selected, &initial);
    assert_eq!(terminal.points(), sibling.points());
    assert_eq!(terminal.storage(), sibling.storage());
    assert_eq!(terminal.diagnostics(), sibling.diagnostics());
    let witness = terminal
        .detail_height_witness(&selected, Default::default(), until())
        .unwrap();
    assert_eq!(witness.check_terminal(&sibling), Err(Error::Identity));
    let foreign = other_owner
        .select_for_calculate(SLOT, terminal.from(), terminal.requested_end())
        .unwrap();
    assert_eq!(witness.check_query(&foreign), Err(Error::Identity));
    assert_eq!(
        terminal
            .detail_height_witness(&foreign, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Boundary(Error::Identity)
    );
    let unrelated = owner
        .select_for_calculate(SLOT, point(5., 7., 0.), terminal.requested_end())
        .unwrap();
    assert_eq!(witness.check_query(&unrelated), Err(Error::Identity));
    assert_eq!(
        terminal
            .detail_height_witness(&unrelated, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Boundary(Error::Identity)
    );
}

#[test]
fn witness_retains_query_and_token_without_retaining_terminal_or_ancestors() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = Rc::new(cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
    ));
    let prior_object = Rc::downgrade(&initial);
    let prior_token = Rc::downgrade(&initial.identity);
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let terminal = Rc::new(straight(&selected, &initial));
    let terminal_object = Rc::downgrade(&terminal);
    let terminal_token = Rc::downgrade(&terminal.identity);
    let witness = terminal
        .detail_height_witness(&selected, Default::default(), until())
        .unwrap();
    drop(initial);
    drop(terminal);
    assert!(prior_object.upgrade().is_none());
    assert!(terminal_object.upgrade().is_none());
    assert!(prior_token.upgrade().is_none());
    assert!(terminal_token.upgrade().is_some());
    drop(owner);
    drop(source);
    let mut retained = witness.query.retain_default();
    let selected = retained
        .select_for_calculate(SLOT, witness.from, witness.to)
        .unwrap();
    assert_eq!(witness.check_query(&selected), Ok(()));
    assert_eq!(selected.query_capacity(), 1024);
    drop(witness);
    assert!(terminal_token.upgrade().is_none());
}

#[test]
fn empty_calls_are_empty_attribution_and_genuine_invalid_height_is_refused() {
    let source = query();
    let mut owner = source.retain_default();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
        .unwrap();
    let initial = cold(&selected);
    let same = straight(&selected, &initial);
    let empty = same
        .detail_height_witness(
            &selected,
            SourceRetainedDetailBudget {
                height_records: 0,
                triangle_tests: 0,
            },
            until(),
        )
        .unwrap();
    assert!(empty.records().is_empty());
    assert_eq!(empty.triangle_tests(), 0);
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(25., 5., 2.))
        .unwrap();
    let boundary = selected
        .loaded_retained_straight_from_same_polygon(
            &initial,
            CONFIG,
            0_f32.to_bits(),
            Default::default(),
            until(),
        )
        .unwrap();
    assert_eq!(
        boundary.diagnostics().heights[0].status,
        (1 << 31) | (1 << 3)
    );
    assert_eq!(
        boundary
            .detail_height_witness(&selected, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Unsupported {
            record: 0,
            reason: SourceRetainedDetailUnsupported::RecordedStatus
        }
    );
}

fn ridge_case() -> (SourceMapQuery, Vec<u8>, [u32; 3], [u32; 3]) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/native-navigation/fixtures");
    let params = std::fs::read(root.join("ridge_wmo/000.mmap")).unwrap();
    let bytes = std::fs::read(root.join("ridge_wmo/0003130.mmtile")).unwrap();
    let scalar: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("scalar-nearest/ridge_wmo.json")).unwrap())
            .unwrap();
    let measured = |index: usize| {
        let p = scalar["queries"][index]["point_bits"].as_array().unwrap();
        let detour: [u32; 3] =
            std::array::from_fn(|i| u32::from_str_radix(p[i].as_str().unwrap(), 16).unwrap());
        [detour[2], detour[0], detour[1]]
    };
    // Existing original scalar positions on the ridge, one row apart. Their
    // use here tests a genuine new composition, not a source sequence oracle.
    let from = measured(2);
    let to = measured(7);
    let source = SourceMapQuery::load(
        SLOT,
        &params,
        &[TileInput {
            grid: [31, 30],
            bytes: &bytes,
        }],
    )
    .unwrap();
    (source, bytes, from, to)
}

#[test]
fn existing_original_generated_ridge_preserves_genuine_invalid_height_refusal() {
    let (source, _, from, to) = ridge_case();
    let mut owner = source.retain_default();
    let initial = cold(&owner.select_for_calculate(SLOT, from, from).unwrap());
    let selected = owner.select_for_calculate(SLOT, from, to).unwrap();
    let terminal = straight(&selected, &initial);
    let snapshot = format!("{terminal:?}");
    assert!(terminal.diagnostics().heights[..9]
        .iter()
        .all(|record| record.status == crate::SUCCESS));
    let invalid = terminal.diagnostics().heights[9];
    eprintln!("genuine ridge record 9: {invalid:?}");
    assert_eq!(invalid.status, (1 << 31) | (1 << 3));
    assert_eq!(invalid.before, invalid.after);
    assert_eq!(
        terminal
            .detail_height_witness(&selected, Default::default(), until())
            .unwrap_err(),
        SourceRetainedDetailError::Unsupported {
            record: 9,
            reason: SourceRetainedDetailUnsupported::RecordedStatus,
        }
    );
    assert_eq!(format!("{terminal:?}"), snapshot);
}

#[test]
fn fixed_shorter_genuine_ridge_calculation_attributes_extra_detail_vertices() {
    let (source, bytes, from, to) = ridge_case();
    let mut owner = source.retain_default();
    let initial = cold(&owner.select_for_calculate(SLOT, from, from).unwrap());
    let selected = owner.select_for_calculate(SLOT, from, to).unwrap();
    let long = straight(&selected, &initial);
    // One fixed actual returned point, not a search for passing geometry and not
    // a filtered copy of height records. This is a separate authored composition
    // on the existing original generated fixture, not original sequence parity.
    let shorter_end = long.points()[8];
    assert_eq!(shorter_end, long.diagnostics().heights[7].after);
    let selected = owner.select_for_calculate(SLOT, from, shorter_end).unwrap();
    let terminal = straight(&selected, &initial);
    assert!(terminal
        .diagnostics()
        .heights
        .iter()
        .all(|record| record.status == crate::SUCCESS));
    let witness = terminal
        .detail_height_witness(&selected, Default::default(), until())
        .unwrap();
    assert!(!witness.records().is_empty());
    assert!(witness
        .records()
        .iter()
        .flat_map(|r| r.vertices)
        .any(|v| v.source == SourceDetailVertexSource::Detail));
    for record in witness.records() {
        assert_serialized_triangle(record, &bytes);
    }
    assert_eq!(
        witness.triangle_tests(),
        witness
            .records()
            .iter()
            .map(|r| 2 * (r.triangle_index as usize + 1))
            .sum::<usize>()
    );
}
