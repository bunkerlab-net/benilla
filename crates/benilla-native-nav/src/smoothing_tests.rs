// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded, source-written ground geometry and actual pinned native queries only.
//! This exercises the original smoothing methods, not owner PathFinder, OptimizePath,
//! NormalizeZ, cached-path reuse, or body/support/gameplay admission. Fresh bounded
//! nearest-poly lookup is a preparation dependency; cold getPolyByLocation is excluded.
use super::*;
use std::time::Duration;

const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const LEFT: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
const RIGHT: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
const GROUND: SourceOwnerFilter = SourceOwnerFilter {
    include_flags: 1,
    exclude_flags: 0,
};
const SOURCE_FAILURE: u32 = 1 << 31;

unsafe extern "C" {
    fn benilla_source_fixup(
        path: *mut u64,
        count: u32,
        capacity: u32,
        visited: *const u64,
        visited_count: u32,
    ) -> u32;
    fn benilla_source_in_range(a: *const f32, b: *const f32, radius: f32, height: f32) -> u32;
}

fn existing_mesh() -> Mesh {
    Mesh::load(
        PARAMS,
        &[
            TileInput {
                grid: [32, 32],
                bytes: LEFT,
            },
            TileInput {
                grid: [32, 31],
                bytes: RIGHT,
            },
        ],
    )
    .unwrap()
}

fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}

fn flat_endpoints() -> ([u32; 3], [u32; 3]) {
    (point(-400., -400., 0.), point(-400., -200., 0.))
}

struct OracleRow<'a> {
    headings: Vec<&'a str>,
    values: Vec<&'a str>,
}

impl OracleRow<'_> {
    fn get(&self, name: &str) -> &str {
        let at = self
            .headings
            .iter()
            .position(|field| *field == name)
            .unwrap();
        self.values[at]
    }

    fn hex(&self, name: &str) -> u32 {
        u32::from_str_radix(self.get(name), 16).unwrap()
    }

    fn count(&self, name: &str) -> usize {
        self.get(name).parse().unwrap()
    }

    fn point(&self, name: &str) -> [u32; 3] {
        parse_point(self.get(name))
    }

    fn refs(&self, name: &str) -> Vec<u64> {
        parse_refs(self.get(name))
    }

    fn nearest(&self, prefix: &str) -> Nearest {
        Nearest {
            polygon: self.get(&format!("{prefix}_ref")).parse().unwrap(),
            point: self.point(&format!("{prefix}_nearest")),
            over: match self.get(&format!("{prefix}_over")) {
                "0" => false,
                "1" => true,
                other => panic!("invalid source over flag: {other}"),
            },
        }
    }
}

fn parse_point(text: &str) -> [u32; 3] {
    let values: Vec<_> = text
        .split(',')
        .map(|value| u32::from_str_radix(value, 16).unwrap())
        .collect();
    values.try_into().unwrap()
}

fn parse_refs(text: &str) -> Vec<u64> {
    if text == "-" {
        Vec::new()
    } else {
        text.split(',')
            .map(|value| value.parse().unwrap())
            .collect()
    }
}

fn oracle_rows() -> Vec<OracleRow<'static>> {
    let fixture = include_str!("../tools/fixtures/smoothing/cases.test-fixture");
    let headings: Vec<_> = fixture
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .expect("missing source oracle schema");
    fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            assert_eq!(values.len(), headings.len(), "oracle row schema: {line}");
            OracleRow {
                headings: headings.clone(),
                values,
            }
        })
        .collect()
}

fn oracle_mesh(geometry: &str) -> Mesh {
    match geometry {
        "existing" => existing_mesh(),
        "ground" => Mesh::load(
            include_bytes!("../tools/fixtures/smoothing/params.test-fixture"),
            &[TileInput {
                grid: [32, 32],
                bytes: include_bytes!("../tools/fixtures/smoothing/ground.test-fixture"),
            }],
        )
        .unwrap(),
        "thin" => Mesh::load(
            include_bytes!("../tools/fixtures/smoothing/params.test-fixture"),
            &[TileInput {
                grid: [32, 32],
                bytes: include_bytes!("../tools/fixtures/smoothing/thin.test-fixture"),
            }],
        )
        .unwrap(),
        other => panic!("unknown authored source geometry {other}"),
    }
}

#[test]
fn loaded_original_methods_match_status_corridor_termination_diagnostics_and_point_bits() {
    let mut labels = std::collections::BTreeSet::new();
    let mut matched = 0;
    let mut source_failures = 0;
    let mut early_stops = 0;
    let mut rejected_corridors = 0;
    let mut carried_height_failures = 0;
    let mut buffered_moves = 0;
    let mut cross_tiles = 0;
    let mut multi_corner_steering = 0;
    for row in oracle_rows() {
        let label = row.get("label");
        assert!(
            labels.insert(label.to_owned()),
            "duplicate source case {label}"
        );
        let from = row.point("from");
        let to = row.point("to");
        let limit = row.count("limit");
        let mut mesh = oracle_mesh(row.get("geometry"));
        let start = row.nearest("from");
        let end = row.nearest("to");
        for (prefix, input, expected) in [("from", from, start), ("to", to, end)] {
            assert_eq!(row.hex(&format!("{prefix}_status")), SUCCESS, "{label}");
            let attempts = row.count(&format!("{prefix}_attempts"));
            assert!((1..=2).contains(&attempts), "{label}");
            if attempts == 2 {
                assert_eq!(
                    mesh.nearest(input, 5., until()),
                    Err(Error::Unavailable),
                    "{label}"
                );
            }
            let extent = if attempts == 1 { 5. } else { 10. };
            assert_eq!(
                mesh.nearest(input, extent, until()),
                Ok(expected),
                "{label}"
            );
        }

        let corridor = mesh.loaded_source_corridor(
            from,
            to,
            GROUND,
            limit,
            SourceSmoothingBudget::default(),
            until(),
        );
        let corridor_status = row.hex("corridor_status");
        if corridor_status != SUCCESS {
            let failure = corridor.expect_err("incomplete source corridor was promoted");
            // Direct finalization preserves its source detail flags, including
            // genuine truncation; operational work cancellation is separate.
            assert_eq!(failure.error, Error::Native(corridor_status), "{label}");
            assert_eq!(failure.diagnostics.primitive_calls, 0, "{label}");
            assert_eq!(row.get("stop"), "corridor_rejected", "{label}");
            rejected_corridors += 1;
            continue;
        }
        let corridor = corridor.unwrap();
        let polygons = row.refs("corridor");
        assert_eq!(corridor.original_from(), from, "{label}");
        assert_eq!(corridor.original_to(), to, "{label}");
        assert_eq!(corridor.filter(), GROUND, "{label}");
        assert_eq!(corridor.point_limit(), limit, "{label}");
        assert_eq!(corridor.start(), start, "{label}");
        assert_eq!(corridor.end(), end, "{label}");
        assert_eq!(corridor.polygons(), polygons.as_slice(), "{label}");
        let result = corridor.smooth();

        let actual = result.unwrap();
        let points: Vec<_> = row.get("points").split(';').map(parse_point).collect();
        let stop = match row.get("stop") {
            "target_reached" => SourceSmoothStop::TargetReached,
            "no_steer_target" => SourceSmoothStop::NoSteerTarget,
            "point_limit" => SourceSmoothStop::PointLimit,
            other => panic!("unexpected admitted source stop {other}: {label}"),
        };
        assert_eq!(actual.from, from, "{label}");
        assert_eq!(actual.to, to, "{label}");
        assert_eq!(actual.filter, GROUND, "{label}");
        assert_eq!(actual.point_limit, limit, "{label}");
        assert_eq!(actual.start, start, "{label}");
        assert_eq!(actual.end, end, "{label}");
        assert_eq!(actual.polygons, polygons, "{label}");
        assert_eq!(actual.points, points, "{label}");
        assert_eq!(actual.status, row.hex("smooth_status"), "{label}");
        assert_eq!(actual.stop, stop, "{label}");
        assert_eq!(
            actual.points.first(),
            Some(&row.point("bounded_from")),
            "{label}"
        );
        if stop == SourceSmoothStop::TargetReached {
            assert_eq!(
                actual.points.last(),
                Some(&row.point("bounded_to")),
                "{label}"
            );
        }
        let source_storage = row.refs("final_storage");
        assert_eq!(source_storage.len(), SOURCE_SMOOTHING_MAX_POINTS, "{label}");
        assert!(!actual.final_polygons.is_empty(), "{label}");
        assert_eq!(
            actual.final_polygons,
            source_storage[..actual.final_polygons.len()],
            "{label}"
        );
        assert_eq!(actual.final_polygons, row.refs("final_corridor"), "{label}");
        assert_eq!(
            actual.diagnostics.boundary_start_status,
            Some(SUCCESS),
            "{label}"
        );
        assert_eq!(
            actual.diagnostics.boundary_end_status,
            Some(SUCCESS),
            "{label}"
        );
        assert_eq!(row.count("boundary_calls"), 2, "{label}");
        assert_eq!(row.hex("boundary_status"), SUCCESS, "{label}");
        for (name, status) in [
            ("steer", actual.diagnostics.steer_status),
            ("move", actual.diagnostics.move_status),
            ("height", actual.diagnostics.height_status),
        ] {
            let expected = (row.count(&format!("{name}_calls")) > 0)
                .then(|| row.hex(&format!("{name}_status")));
            assert_eq!(status, expected, "{label}: {name}");
        }
        let primitive_calls: usize = [
            "boundary_calls",
            "steer_calls",
            "move_calls",
            "height_calls",
        ]
        .into_iter()
        .map(|name| row.count(name))
        .sum();
        assert_eq!(
            actual.diagnostics.primitive_calls, primitive_calls,
            "{label}"
        );
        assert_eq!(
            actual.diagnostics.iterations,
            row.count("steer_calls"),
            "{label}"
        );
        assert_eq!(
            actual.diagnostics.steer_buffer_too_small,
            row.count("steer_buffer_calls"),
            "{label}"
        );
        assert_eq!(
            actual.diagnostics.height_failures,
            row.count("height_failures"),
            "{label}"
        );
        assert_eq!(
            actual.diagnostics.move_buffer_too_small,
            row.count("move_buffer_calls"),
            "{label}"
        );
        if row.count("full_steer_count") > 3 {
            assert_eq!(row.hex("full_steer_status"), SUCCESS, "{label}");
            assert_ne!(row.hex("probe_steer_status") & 16, 0, "{label}");
            assert_eq!(row.count("probe_steer_count"), 3, "{label}");
            assert!(actual.diagnostics.steer_buffer_too_small > 0, "{label}");
            multi_corner_steering += 1;
        }
        matched += 1;
        source_failures += usize::from(actual.status == SOURCE_FAILURE);
        early_stops += usize::from(stop == SourceSmoothStop::NoSteerTarget);
        cross_tiles += usize::from(row.get("geometry") == "existing");
        // Original getPolyHeight failure preserves the Y already initialized by
        // successful moveAlongSurface; the source still adds its binary32 +0.5.
        carried_height_failures += usize::from(row.count("height_failures") > 0);
        buffered_moves += usize::from(row.count("move_buffer_calls") > 0);
    }
    for required in [
        "flat_vertical_offsets",
        "flat_target_boundary_clamp",
        "flat_start_boundary_clamp",
        "far_lookup",
        "coincident_early_stop",
        "slop_early_stop",
        "flat_limit2",
        "short_reaches_at_limit2",
        "slope_multi_portal",
        "slope_vertical_offsets",
        "bend_many_corners",
        "bend_reverse",
        "long_limit74",
        "unreachable_partial_corridor",
        "cross_tile_unsnapped_vertical",
        "exact_fit_two_polygons_limit2",
        "thin_visited_buffer",
        "thin_visited_buffer_reverse",
    ] {
        assert!(
            labels.contains(required),
            "missing source coverage: {required}"
        );
    }
    assert!(matched >= 15, "only {matched} exact loaded source cases");
    assert!(source_failures >= 4, "point-limit cases: {source_failures}");
    assert!(early_stops >= 2, "early-stop cases: {early_stops}");
    assert!(
        rejected_corridors >= 1,
        "partial source corridor coverage missing"
    );
    assert!(
        carried_height_failures >= 1,
        "defined source height-failure carry missing"
    );
    assert!(
        buffered_moves >= 2,
        "defined source visited-buffer continuation missing"
    );
    assert!(cross_tiles >= 3, "cross-tile exact cases: {cross_tiles}");
    assert!(
        multi_corner_steering >= 2,
        "long-funnel exact cases: {multi_corner_steering}"
    );
}

#[test]
fn source_fixup_preserves_duplicate_scan_reverse_prefix_truncation_and_limit_guards() {
    let mut labels = std::collections::BTreeSet::new();
    for line in include_str!("../tools/fixtures/smoothing/fixup.test-fixture").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let label = fields[0];
        assert!(labels.insert(label));
        let limit: u32 = fields[1].parse().unwrap();
        let original = parse_refs(fields[2]);
        let visited = parse_refs(fields[3]);
        let expected = parse_refs(fields[4]);
        assert!(original.len() <= 74 && visited.len() <= 16);
        assert!((2..=74).contains(&limit));
        const GUARD: u64 = 0xfeed_face_cafe_beef;
        let mut guarded = [GUARD; 76];
        guarded[1..1 + original.len()].copy_from_slice(&original);
        // SAFETY: 74 writable numeric slots enclosed by sentinels, bounded input
        // slices, and no references from this pure helper ever reach a mesh query.
        let count = unsafe {
            benilla_source_fixup(
                guarded[1..].as_mut_ptr(),
                original.len() as u32,
                limit,
                visited.as_ptr(),
                visited.len() as u32,
            )
        } as usize;
        assert!(count <= 74, "{label}");
        assert_eq!(&guarded[1..1 + count], expected.as_slice(), "{label}");
        assert_eq!(guarded[0], GUARD, "{label}");
        assert_eq!(guarded[75], GUARD, "{label}");
    }
    assert_eq!(labels.len(), 9);
    for label in [
        "no_intersection",
        "reverse_visited",
        "earliest_duplicate_visited",
        "furthest_duplicate_path",
        "truncate_tail",
        "req_equals_limit",
        "req_exceeds_limit",
        "empty_visited",
        "single_common",
    ] {
        assert!(labels.contains(label));
    }
}

#[test]
fn strict_source_range_thresholds_match_original_binary32_helpers() {
    let mut labels = std::collections::BTreeSet::new();
    for line in include_str!("../tools/fixtures/smoothing/range.test-fixture").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 6);
        let label = fields[0];
        assert!(labels.insert(label));
        let a = detour(parse_point(fields[1])).unwrap();
        let b = detour(parse_point(fields[2])).unwrap();
        let radius = f32::from_bits(u32::from_str_radix(fields[3], 16).unwrap());
        let height = f32::from_bits(u32::from_str_radix(fields[4], 16).unwrap());
        let expected: u32 = fields[5].parse().unwrap();
        // SAFETY: initialized finite triples and finite nonnegative thresholds; helper
        // is numerical only and reads exactly three floats from each array.
        let actual = unsafe { benilla_source_in_range(a.as_ptr(), b.as_ptr(), radius, height) };
        assert_eq!(actual, expected, "{label}");
    }
    assert_eq!(labels.len(), 12);
}

#[test]
fn original_vertical_endpoints_are_not_nearest_projection_or_owner_normalization() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 2.5);
    let to = point(-400., -200., 3.25);
    let smooth = mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(smooth.from, from);
    assert_eq!(smooth.to, to);
    assert_eq!(smooth.start.point[2], 0f32.to_bits());
    assert_eq!(smooth.end.point[2], 0f32.to_bits());
    assert_eq!(smooth.points.first(), Some(&from));
    assert_eq!(smooth.points.last(), Some(&to));
    assert_eq!(smooth.status, SUCCESS);
    assert_eq!(smooth.stop, SourceSmoothStop::TargetReached);
    assert!(smooth.points.len() > 3);
    for sample in &smooth.points[1..smooth.points.len() - 1] {
        assert_eq!(sample[2], 0.5f32.to_bits());
    }
}

#[test]
fn no_steer_source_success_is_not_promoted_to_a_complete_route() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 2.5);
    let to = point(-400., -400., 3.25);
    let smooth = mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(smooth.status, SUCCESS);
    assert_eq!(smooth.stop, SourceSmoothStop::NoSteerTarget);
    assert_eq!(smooth.points, [from]);
    assert_ne!(smooth.points.last(), Some(&to));
}

#[test]
fn source_point_limit_failure_preserves_raw_points_without_claiming_success() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    let smooth = mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            2,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(smooth.status, SOURCE_FAILURE);
    assert_eq!(smooth.stop, SourceSmoothStop::PointLimit);
    assert_eq!(smooth.points.len(), 2);
    assert_eq!(smooth.points[0], from);
    assert_ne!(smooth.points[1], to);
    assert_eq!(smooth.points[1][2], 0.5f32.to_bits());
}

#[test]
fn smoothing_work_exhaustion_never_returns_partial_output_and_query_reuse_is_clean() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    let legacy = mesh.route(from, to, until()).unwrap();
    let complete = mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    for limit in 0..=8 {
        let result = mesh.loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget {
                corridor_expansions: 4096,
                smoothing_primitives: limit,
            },
            until(),
        );
        let failure = result.expect_err("work cancellation published partial output");
        assert_eq!(failure.error, Error::Budget, "primitive budget {limit}");
        assert!(failure.diagnostics.primitive_calls <= limit);
        assert_eq!(mesh.route(from, to, until()).unwrap(), legacy);
        let retried = mesh
            .loaded_source_smooth(
                from,
                to,
                GROUND,
                74,
                SourceSmoothingBudget::default(),
                until(),
            )
            .unwrap();
        assert_eq!(retried.status, complete.status);
        assert_eq!(retried.stop, complete.stop);
        assert_eq!(retried.polygons, complete.polygons);
        assert_eq!(retried.points, complete.points);
    }
}

#[test]
fn direct_corridor_cancellation_cleans_up_before_legacy_or_source_reuse() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    let legacy = mesh.route(from, to, until()).unwrap();
    for expansions in [0, 1] {
        let failure = mesh
            .loaded_source_smooth(
                from,
                to,
                GROUND,
                74,
                SourceSmoothingBudget {
                    corridor_expansions: expansions,
                    smoothing_primitives: 256,
                },
                until(),
            )
            .expect_err("cancelled direct corridor unexpectedly returned a path");
        assert_eq!(
            failure.error,
            Error::Budget,
            "expansion budget {expansions}"
        );
        assert_eq!(failure.diagnostics.primitive_calls, 0);
        assert_eq!(mesh.route(from, to, until()).unwrap(), legacy);
        let complete = mesh
            .loaded_source_smooth(
                from,
                to,
                GROUND,
                74,
                SourceSmoothingBudget::default(),
                until(),
            )
            .unwrap();
        assert_eq!(complete.status, SUCCESS);
        assert_eq!(complete.stop, SourceSmoothStop::TargetReached);
    }
}

#[test]
fn original_deadline_is_retained_between_corridor_preparation_and_smoothing() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    let expired = mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            Instant::now(),
        )
        .expect_err("expired request published a result");
    assert_eq!(expired.error, Error::Deadline);

    let deadline = Instant::now() + Duration::from_secs(1);
    let prepared = mesh
        .loaded_source_corridor(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            deadline,
        )
        .unwrap();
    assert_eq!(prepared.original_from(), from);
    assert_eq!(prepared.original_to(), to);
    assert_eq!(prepared.filter(), GROUND);
    assert_eq!(prepared.point_limit(), 74);
    assert_eq!(prepared.polygons().len(), 2);
    assert_eq!(prepared.start().point, from);
    assert_eq!(prepared.end().point, to);
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    let failure = prepared
        .smooth()
        .expect_err("prepared handle refreshed its caller's expired deadline");
    assert_eq!(failure.error, Error::Deadline);
    assert_eq!(failure.diagnostics.primitive_calls, 0);
    assert!(mesh.route(from, to, until()).is_ok());
    assert!(mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until()
        )
        .is_ok());
}

#[test]
fn unsupported_owner_masks_and_invalid_limits_reject_before_smoothing() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    for (include_flags, exclude_flags) in [
        (0, 0),
        (2, 0),
        (3, 0),
        (4, 0),
        (12, 0),
        (15, 0),
        (13, 1),
        (13, 4),
        (1, 1),
        (1, 2),
        (u16::MAX, 0),
        (1, u16::MAX),
    ] {
        let filter = SourceOwnerFilter {
            include_flags,
            exclude_flags,
        };
        let failure = mesh
            .loaded_source_smooth(
                from,
                to,
                filter,
                74,
                SourceSmoothingBudget::default(),
                until(),
            )
            .expect_err("unsupported owner filter was silently replaced with ground");
        assert_eq!(failure.error, Error::Unsupported);
        assert_eq!(failure.diagnostics.primitive_calls, 0);
    }
    for limit in [0, 1, 75, usize::MAX] {
        let failure = mesh
            .loaded_source_smooth(
                from,
                to,
                GROUND,
                limit,
                SourceSmoothingBudget::default(),
                until(),
            )
            .expect_err("point limit escaped the admitted 2..=74 range");
        assert_eq!(failure.error, Error::Format, "point limit {limit}");
        assert_eq!(failure.diagnostics.primitive_calls, 0);
    }
    for budget in [
        SourceSmoothingBudget {
            corridor_expansions: 4097,
            smoothing_primitives: 256,
        },
        SourceSmoothingBudget {
            corridor_expansions: 4096,
            smoothing_primitives: 257,
        },
        SourceSmoothingBudget {
            corridor_expansions: usize::MAX,
            smoothing_primitives: usize::MAX,
        },
    ] {
        let failure = mesh
            .loaded_source_smooth(from, to, GROUND, 74, budget, until())
            .expect_err("oversized safety budget was admitted");
        assert_eq!(failure.error, Error::Budget);
        assert_eq!(failure.diagnostics.primitive_calls, 0);
    }
    assert!(mesh.route(from, to, until()).is_ok());
}

#[test]
fn nonfinite_out_of_range_and_unavailable_original_endpoints_reject() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    for value in [
        f32::NAN.to_bits(),
        0x7f80_0001,
        f32::INFINITY.to_bits(),
        f32::NEG_INFINITY.to_bits(),
        f32::MAX.to_bits(),
        (-f32::MAX).to_bits(),
        100_001f32.to_bits(),
        (-100_001f32).to_bits(),
    ] {
        for axis in 0..3 {
            for corrupt_start in [false, true] {
                let mut a = from;
                let mut b = to;
                if corrupt_start {
                    a[axis] = value;
                } else {
                    b[axis] = value;
                }
                let failure = mesh
                    .loaded_source_smooth(
                        a,
                        b,
                        GROUND,
                        74,
                        SourceSmoothingBudget::default(),
                        until(),
                    )
                    .expect_err("invalid original endpoint reached the native smoother");
                assert_eq!(failure.error, Error::Format);
                assert_eq!(failure.diagnostics.primitive_calls, 0);
            }
        }
    }
    let too_far = mesh
        .loaded_source_smooth(
            from,
            point(5000., 5000., 0.),
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .expect_err("oversized route extent was admitted");
    assert_eq!(too_far.error, Error::Budget);
    let unavailable = mesh
        .loaded_source_smooth(
            from,
            point(1000., 1000., 0.),
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .expect_err("missing ground endpoint produced a smooth result");
    assert_eq!(unavailable.error, Error::Unavailable);
    assert!(mesh.route(from, to, until()).is_ok());
}

#[test]
fn fresh_near_far_preparation_preserves_the_source_lookup_and_original_endpoint() {
    let rows = oracle_rows();
    let row = rows
        .iter()
        .find(|row| row.get("label") == "far_lookup")
        .unwrap();
    let mut mesh = oracle_mesh(row.get("geometry"));
    let from = row.point("from");
    let to = row.point("to");
    assert_eq!(row.count("from_attempts"), 2);
    assert_eq!(row.count("to_attempts"), 1);
    assert_eq!(mesh.nearest(from, 5., until()), Err(Error::Unavailable));
    let start = mesh.nearest(from, 10., until()).unwrap();
    let end = mesh.nearest(to, 5., until()).unwrap();
    let smooth = mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(smooth.start, start);
    assert_eq!(smooth.end, end);
    assert_eq!(smooth.from, from);
    assert_eq!(smooth.to, to);
    assert_ne!(smooth.start.point, from);
    assert_eq!(smooth.points.first(), Some(&row.point("bounded_from")));
    assert_eq!(smooth.points.last(), Some(&row.point("bounded_to")));
    assert_eq!(smooth.status, SUCCESS);
    assert_eq!(smooth.stop, SourceSmoothStop::TargetReached);
}

#[test]
fn existing_route_keeps_its_snapped_funnel_semantics_after_source_smoothing() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 2.5);
    let to = point(-400., -200., 3.25);
    let before = mesh.route(from, to, until()).unwrap();
    assert_eq!(before.corners.first().unwrap().point[2], 0f32.to_bits());
    assert_eq!(before.corners.last().unwrap().point[2], 0f32.to_bits());
    let source = mesh
        .loaded_source_smooth(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert!(source.points.len() > before.corners.len());
    assert_eq!(source.points.first(), Some(&from));
    assert_eq!(source.points.last(), Some(&to));
    assert_eq!(mesh.route(from, to, until()).unwrap(), before);
}

#[test]
fn dropping_prepared_handle_releases_its_exclusive_mesh_without_cached_corridor_reuse() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    let prepared = mesh
        .loaded_source_corridor(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(prepared.original_from(), from);
    assert_eq!(prepared.original_to(), to);
    drop(prepared);
    let reverse = mesh
        .loaded_source_smooth(
            to,
            from,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(reverse.points.first(), Some(&to));
    assert_eq!(reverse.points.last(), Some(&from));
    assert_eq!(reverse.polygons.first(), Some(&reverse.start.polygon));
    assert_eq!(reverse.polygons.last(), Some(&reverse.end.polygon));
    assert_eq!(reverse.stop, SourceSmoothStop::TargetReached);
}

#[test]
fn swimming_capable_dry_filter_remains_thirteen_through_smoothing_and_warm_seed() {
    let mut mesh = existing_mesh();
    let (from, to) = flat_endpoints();
    let filter = SourceOwnerFilter {
        include_flags: 13,
        exclude_flags: 0,
    };
    let corridor = mesh
        .loaded_source_corridor(
            from,
            to,
            filter,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(corridor.filter(), filter);
    let seed = corridor.warm_cache_seed().unwrap();
    assert_eq!(seed.filter(), filter);
    let smooth = corridor.smooth().unwrap();
    assert_eq!(smooth.filter, filter);
}

#[test]
fn supporting_filter_thirteen_does_not_admit_water_or_magma_polygons() {
    let vertex_count = u32::from_le_bytes(LEFT[48..52].try_into().unwrap()) as usize;
    let poly = 20 + 100 + vertex_count * 12;
    for (flags, area) in [(4u16, 9u8), (8, 10), (13, 11), (1, 9)] {
        let mut wet = LEFT.to_vec();
        wet[poly + 28..poly + 30].copy_from_slice(&flags.to_le_bytes());
        wet[poly + 31] = area;
        assert!(matches!(
            Mesh::load(
                PARAMS,
                &[TileInput {
                    grid: [32, 32],
                    bytes: &wet,
                }]
            ),
            Err(Error::Unsupported)
        ));
    }
}
