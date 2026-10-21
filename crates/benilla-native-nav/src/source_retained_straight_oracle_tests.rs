// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent original same-object source observations, replayed from genuine cold input.
//! Missing generated products fail compilation; no authored cache seed or fallback exists.
use super::*;
use crate::{
    SourceSamePolygonBudget, SourceStraightHeight, SourceWarmLookupOrigin, SourceWarmPrimitive,
    TileInput,
};
use std::time::Duration;

const CASE_FIELDS: &str = "label sequence step oracle query_nodes include_filter exclude_filter \
    point_limit after_start_storage geometry scope from to collision_width optimize normalize \
    ignore force previous_type before_active before_storage start_ref start_distance end_ref \
    end_distance lookup_storage phase ray_status ray_hit ray_normal ray_active ray_storage \
    adjusted_hit width_calls after_active after_storage path_type end actual_end points \
    height_calls height_failures";
const TRACE_FIELDS: &str =
    "label ordinal primitive ref target status input output scalar count refs";
const CASES: &str = include_str!("../tools/fixtures/retained-straight-origin/cases.test-fixture");
const TRACES: &str = include_str!("../tools/fixtures/retained-straight-origin/traces.test-fixture");
const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};

struct Row {
    headings: Vec<&'static str>,
    values: Vec<&'static str>,
}
impl Row {
    fn get(&self, key: &str) -> &'static str {
        self.values[self
            .headings
            .iter()
            .position(|field| *field == key)
            .unwrap_or_else(|| panic!("missing source-origin column {key}"))]
    }
    fn count(&self, key: &str) -> usize {
        self.get(key).parse().unwrap()
    }
    fn reference(&self, key: &str) -> u64 {
        self.get(key).parse().unwrap()
    }
    fn hex(&self, key: &str) -> u32 {
        let value = self.get(key);
        assert_eq!(value.len(), 8, "source hex column {key}");
        u32::from_str_radix(value, 16).unwrap()
    }
    fn optional_hex(&self, key: &str) -> Option<u32> {
        (self.get(key) != "-").then(|| self.hex(key))
    }
    fn flag(&self, key: &str) -> bool {
        match self.get(key) {
            "0" => false,
            "1" => true,
            other => panic!("invalid source flag {key}: {other}"),
        }
    }
    fn refs(&self, key: &str) -> Vec<u64> {
        match self.get(key) {
            "-" => Vec::new(),
            values => values
                .split(',')
                .map(|value| value.parse().unwrap())
                .collect(),
        }
    }
    fn points(&self, key: &str) -> Vec<[u32; 3]> {
        match self.get(key) {
            "-" => Vec::new(),
            values => values.split(';').map(parse_point).collect(),
        }
    }
    fn point(&self, key: &str) -> [u32; 3] {
        parse_point(self.get(key))
    }
    fn filter(&self) -> SourceOwnerFilter {
        SourceOwnerFilter {
            include_flags: self.count("include_filter").try_into().unwrap(),
            exclude_flags: self.count("exclude_filter").try_into().unwrap(),
        }
    }
    fn config(&self) -> SourceRetainedStraightConfig {
        SourceRetainedStraightConfig {
            normalization: SourceSamePolygonConfig {
                normalize_z: self.flag("normalize"),
                ignore_normalization: self.flag("ignore"),
            },
            optimize_path: self.flag("optimize"),
        }
    }
    fn phase(&self) -> SourceWarmPolyPhase {
        match self.get("phase") {
            "same" => SourceWarmPolyPhase::SamePolygon,
            "cut" => SourceWarmPolyPhase::Cut,
            "replacement" => SourceWarmPolyPhase::Replacement,
            other => panic!("invalid source phase {other}"),
        }
    }
}
fn parse_point(value: &str) -> [u32; 3] {
    value
        .split(',')
        .map(|word| {
            assert_eq!(word.len(), 8, "source point word");
            u32::from_str_radix(word, 16).unwrap()
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}
fn rows(fixture: &'static str, fields: &'static str) -> Vec<Row> {
    let headers: Vec<_> = fixture
        .lines()
        .filter_map(|line| line.strip_prefix("# label\t"))
        .collect();
    assert_eq!(headers.len(), 1, "one source-origin schema required");
    let headings: Vec<_> = std::iter::once("label")
        .chain(headers[0].split('\t'))
        .collect();
    assert_eq!(
        headings,
        fields.split_whitespace().collect::<Vec<_>>(),
        "source-origin schema drift"
    );
    let rows: Vec<_> = fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            assert_eq!(values.len(), headings.len(), "source-origin row {line}");
            Row {
                headings: headings.clone(),
                values,
            }
        })
        .collect();
    assert!(!rows.is_empty(), "source-origin product must not be empty");
    rows
}
fn source_query() -> SourceMapQuery {
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
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn compare_selection(
    row: &Row,
    before: &[u64],
    start: SourceWarmLookup,
    end: SourceWarmLookup,
    phase: SourceWarmPolyPhase,
) {
    let label = row.get("label");
    assert_eq!(
        start.polygon,
        row.reference("start_ref"),
        "start ref {label}"
    );
    assert_eq!(
        start.distance_bits,
        row.hex("start_distance"),
        "start distance {label}"
    );
    assert_eq!(end.polygon, row.reference("end_ref"), "end ref {label}");
    assert_eq!(
        end.distance_bits,
        row.hex("end_distance"),
        "end distance {label}"
    );
    assert_eq!(phase, row.phase(), "phase {label}");
    // These intermediate snapshots are derived from the exposed ordered append decisions,
    // not separately exposed native storage snapshots or inputs to the loaded calculation.
    let mut storage = before.to_vec();
    for (lookup, column) in [(start, "after_start_storage"), (end, "lookup_storage")] {
        if lookup.appended {
            assert!(matches!(
                lookup.origin,
                SourceWarmLookupOrigin::Nearest5 | SourceWarmLookupOrigin::Nearest10
            ));
            storage.push(lookup.polygon);
        }
        assert_eq!(storage, row.refs(column), "complete {column} {label}");
    }
}

fn compare_lookup_trace(row: &Row, actual: &[SourceWarmTrace], source: &[&Row]) {
    let label = row.get("label");
    // Rust also records CutScan, which makes no Detour call and has no original primitive
    // event. The API exposes lookup and height streams separately, not one interleaving.
    assert!(
        actual.iter().all(|event| matches!(
            event.primitive,
            SourceWarmPrimitive::Active
                | SourceWarmPrimitive::Tail
                | SourceWarmPrimitive::Nearest5
                | SourceWarmPrimitive::Nearest10
                | SourceWarmPrimitive::CutScan
        )),
        "no direct search {label}"
    );
    let actual: Vec<_> = actual
        .iter()
        .filter(|event| event.primitive != SourceWarmPrimitive::CutScan)
        .collect();
    let source: Vec<_> = source
        .iter()
        .filter(|row| matches!(row.get("primitive"), "closest" | "nearest5" | "nearest10"))
        .collect();
    assert_eq!(actual.len(), source.len(), "ordered lookup count {label}");
    let mut previous = None;
    let mut minimum = f32::MAX;
    for (actual, source) in actual.into_iter().zip(source) {
        let name = source.get("label");
        assert_eq!(
            source.get("primitive"),
            match actual.primitive {
                SourceWarmPrimitive::Active | SourceWarmPrimitive::Tail => "closest",
                SourceWarmPrimitive::Nearest5 => "nearest5",
                SourceWarmPrimitive::Nearest10 => "nearest10",
                _ => unreachable!(),
            },
            "primitive {name}"
        );
        assert_eq!(actual.polygon, source.reference("ref"), "lookup ref {name}");
        assert_eq!(actual.status, source.hex("status"), "lookup status {name}");
        assert_eq!(source.reference("target"), 0);
        assert_eq!(source.count("count"), 0);
        assert!(source.refs("refs").is_empty());
        assert_eq!(source.hex("scalar"), 0);
        let input = source.point("input");
        match actual.endpoint {
            0 => assert_eq!(input, row.point("from"), "start lookup input {name}"),
            1 => assert_eq!(input, row.point("to"), "end lookup input {name}"),
            3 | 4 => (),
            other => panic!("unexpected lookup role {other}: {name}"),
        }
        let key = (actual.endpoint, actual.primitive, input);
        if previous != Some(key) {
            minimum = f32::MAX;
        }
        previous = Some(key);
        let output = source.points("output");
        // The public native trace retains distance, not closest-point coordinates. Derive
        // comparison distance from original observer points in exact Detour Y,Z,X order.
        let distance = if actual.status & crate::SUCCESS != 0 && !output.is_empty() {
            assert_eq!(output.len(), 1);
            let a = input.map(f32::from_bits);
            let b = output[0].map(f32::from_bits);
            let y = a[1] - b[1];
            let z = a[2] - b[2];
            let x = a[0] - b[0];
            let squared = y * y + z * z + x * x;
            if source.get("primitive") == "closest" {
                minimum = minimum.min(squared);
                minimum.sqrt()
            } else {
                squared.sqrt()
            }
        } else if source.get("primitive") == "closest" {
            minimum.sqrt()
        } else {
            0.
        };
        assert_eq!(
            actual.distance_bits,
            distance.to_bits(),
            "lookup distance {name}"
        );
    }
}

fn compare_heights(row: &Row, actual: &[SourceStraightHeight], source: &[&Row]) {
    let heights: Vec<_> = source
        .iter()
        .filter(|event| event.get("primitive") == "height")
        .collect();
    assert_eq!(
        actual.len(),
        heights.len(),
        "ordered heights {}",
        row.get("label")
    );
    assert_eq!(actual.len(), row.count("height_calls"));
    assert_eq!(
        actual
            .iter()
            .filter(|event| event.status & (1 << 31) != 0)
            .count(),
        row.count("height_failures")
    );
    for (index, (actual, source)) in actual.iter().zip(heights).enumerate() {
        let name = source.get("label");
        assert_eq!(actual.polygon, source.reference("ref"), "height ref {name}");
        assert_eq!(actual.status, source.hex("status"), "height status {name}");
        assert_eq!(actual.before, source.point("input"), "height input {name}");
        assert_eq!(actual.after, source.point("output"), "height output {name}");
        assert_eq!(
            actual.after[2],
            source.hex("scalar"),
            "height scalar {name}"
        );
        assert_eq!(
            actual.endpoint,
            if index == 0 && row.optional_hex("adjusted_hit").is_some() {
                3
            } else {
                4
            }
        );
        assert_eq!(source.reference("target"), 0);
        assert_eq!(source.count("count"), 0);
        assert!(source.refs("refs").is_empty());
    }
}

fn compare_ray(row: &Row, result: &LoadedSourceRetainedStraight, source: &[&Row]) {
    let label = row.get("label");
    let rays: Vec<_> = source
        .iter()
        .filter(|event| event.get("primitive") == "raycast")
        .collect();
    match result.ray() {
        None => {
            assert!(rays.is_empty(), "ray absent {label}");
            for column in [
                "ray_status",
                "ray_hit",
                "ray_normal",
                "ray_active",
                "ray_storage",
                "adjusted_hit",
            ] {
                assert_eq!(row.get(column), "-", "{column} absent {label}");
            }
            assert_eq!(row.count("width_calls"), 0);
            assert_eq!(result.diagnostics().raycast_polygons, 0);
        }
        Some(ray) => {
            assert_eq!(rays.len(), 1, "one source ray {label}");
            let source = rays[0];
            assert_eq!(ray.status, row.hex("ray_status"), "ray status {label}");
            assert_eq!(ray.hit_bits, row.hex("ray_hit"), "ray hit {label}");
            assert_eq!(
                ray.polygons.len(),
                row.count("ray_active"),
                "ray active {label}"
            );
            assert_eq!(
                ray.adjusted_hit_bits,
                row.optional_hex("adjusted_hit"),
                "adjusted hit {label}"
            );
            assert_eq!(
                usize::from(ray.adjusted_hit_bits.is_some()),
                row.count("width_calls")
            );
            let normal = row.point("ray_normal");
            assert_eq!(
                ray.normal_bits,
                [normal[1], normal[2], normal[0]],
                "ray normal YZX {label}"
            );
            let mut storage = row.refs("lookup_storage");
            storage[..ray.polygons.len()].copy_from_slice(&ray.polygons);
            assert_eq!(
                storage,
                row.refs("ray_storage"),
                "derived complete ray storage {label}"
            );
            assert_eq!(source.reference("ref"), result.start_lookup().polygon);
            assert_eq!(source.reference("target"), 0);
            assert_eq!(source.hex("status"), ray.status);
            assert_eq!(source.hex("scalar"), ray.hit_bits);
            assert_eq!(source.point("output"), normal);
            assert_eq!(source.count("count"), ray.polygons.len());
            assert_eq!(source.refs("refs"), ray.polygons);
            assert_eq!(
                source.points("input"),
                [result.from(), result.requested_end()]
            );
        }
    }
}

fn compare_cold(row: &Row, result: &LoadedSourceSamePolygon, source: &[&Row]) {
    let label = row.get("label");
    let cache = result.compatibility();
    assert_eq!(result.query_capacity() as usize, row.count("query_nodes"));
    assert_eq!(result.config(), row.config().normalization);
    assert_eq!(cache.filter(), row.filter());
    assert_eq!(cache.point_limit(), row.count("point_limit"));
    assert_eq!(cache.initial_storage(), row.refs("before_storage"));
    assert_eq!(cache.before_storage(), row.refs("before_storage"));
    assert_eq!(cache.initial_active_len(), row.count("before_active"));
    assert_eq!(cache.before_active_len(), row.count("before_active"));
    assert_eq!(
        cache.storage(),
        row.refs("after_storage"),
        "cold backing {label}"
    );
    assert_eq!(cache.active_len(), row.count("after_active"));
    assert_eq!(cache.from(), row.point("from"));
    assert_eq!(cache.to(), row.point("to"));
    assert_eq!(cache.to(), row.point("end"));
    assert_eq!(result.actual_end(), row.point("actual_end"));
    assert_eq!(result.path_type() as usize, row.count("path_type"));
    assert_eq!(result.points(), row.points("points"), "cold points {label}");
    compare_selection(
        row,
        cache.before_storage(),
        cache.start(),
        cache.end(),
        cache.phase(),
    );
    compare_lookup_trace(row, cache.trace(), source);
    assert!(source
        .iter()
        .all(|event| matches!(event.get("primitive"), "closest" | "nearest5" | "nearest10")));
    for column in [
        "ray_status",
        "ray_hit",
        "ray_normal",
        "ray_active",
        "ray_storage",
        "adjusted_hit",
    ] {
        assert_eq!(row.get(column), "-");
    }
    assert_eq!(row.count("width_calls"), 0);
    assert_eq!(row.count("height_calls"), 0);
    assert_eq!(row.count("height_failures"), 0);
    // Initial previous_type is a source constructor-domain assertion, not a native
    // getter. Cold's width/optimize inputs are unconsulted and absent from its API.
    assert_eq!(row.count("previous_type"), 0);
}

fn compare_straight(
    row: &Row,
    result: &LoadedSourceRetainedStraight,
    initial: &LoadedSourceSamePolygon,
    source: &[&Row],
) {
    let label = row.get("label");
    assert_eq!(result.query_capacity() as usize, row.count("query_nodes"));
    assert_eq!(result.query_policy(), crate::SOURCE_MAP_QUERY_POLICY);
    assert_eq!(
        result.compatibility_id(),
        initial.compatibility().compatibility_id()
    );
    assert_eq!(
        result.compatibility_version(),
        initial.compatibility().compatibility_version()
    );
    assert_eq!(
        result.source_revision(),
        initial.compatibility().source_revision()
    );
    assert_eq!(
        result.initial_storage(),
        initial.compatibility().initial_storage()
    );
    assert_eq!(
        result.initial_active_len(),
        initial.compatibility().initial_active_len()
    );
    assert_eq!(
        result.before_storage(),
        row.refs("before_storage"),
        "before backing {label}"
    );
    assert_eq!(result.before_active_len(), row.count("before_active"));
    assert_eq!(
        result.storage(),
        row.refs("after_storage"),
        "after backing {label}"
    );
    assert_eq!(result.active_len(), row.count("after_active"));
    assert_eq!(result.filter(), row.filter());
    assert_eq!(result.point_limit(), row.count("point_limit"));
    assert_eq!(result.config(), row.config());
    assert_eq!(result.collision_width_bits(), row.hex("collision_width"));
    assert_eq!(
        result.previous_path_type() as usize,
        row.count("previous_type")
    );
    assert_eq!(result.from(), row.point("from"));
    assert_eq!(result.requested_end(), row.point("to"));
    assert_eq!(result.end(), row.point("end"));
    assert_eq!(result.actual_end(), row.point("actual_end"));
    assert_eq!(result.path_type() as usize, row.count("path_type"));
    assert_eq!(
        result.points(),
        row.points("points"),
        "all point bits {label}"
    );
    compare_selection(
        row,
        result.before_storage(),
        result.start_lookup(),
        result.end_lookup(),
        result.phase(),
    );
    compare_lookup_trace(row, result.trace(), source);
    compare_heights(row, &result.diagnostics().heights, source);
    compare_ray(row, result, source);
    // Ray iterations and terminal work units are operational counters, not original
    // trace fields. Do not reinterpret source ray output capacity as traversal work.
}

#[test]
fn original_cold_origin_sequences_match_retained_straight_storage_types_points_and_traces() {
    let cases = rows(CASES, CASE_FIELDS);
    let traces = rows(TRACES, TRACE_FIELDS);
    assert_eq!(
        cases
            .iter()
            .map(|row| (row.get("label"), row.get("sequence"), row.count("step")))
            .collect::<Vec<_>>(),
        [
            ("normal_cold", "normal", 0),
            ("normal_no_hit", "normal", 1),
            ("incomplete_cold", "incomplete", 0),
            ("incomplete_hit", "incomplete", 1),
            ("incomplete_no_hit", "incomplete", 2),
        ]
    );
    // Both allocations remain simultaneously live. Fixture reference numbers are only
    // compared after execution; they never seed, restore or construct loaded authority.
    let queries = [source_query(), source_query()];
    let mut owners = [queries[0].retain_default(), queries[1].retain_default()];
    let mut initial: [Option<LoadedSourceSamePolygon>; 2] = [None, None];
    let mut previous: [Option<LoadedSourceRetainedStraight>; 2] = [None, None];
    let mut consumed_trace_rows = 0;
    for row in &cases {
        let label = row.get("label");
        let index = match row.get("sequence") {
            "normal" => 0,
            "incomplete" => 1,
            other => panic!("unknown sequence {other}"),
        };
        let events: Vec<_> = traces
            .iter()
            .filter(|event| event.get("label").rsplit_once('_').unwrap().0 == label)
            .collect();
        assert!(!events.is_empty(), "missing original trace {label}");
        for (ordinal, event) in events.iter().enumerate() {
            assert_eq!(event.count("ordinal"), ordinal);
            assert_eq!(event.get("label"), format!("{label}_{ordinal:04}"));
            assert!(matches!(
                event.get("primitive"),
                "closest" | "nearest5" | "nearest10" | "raycast" | "height"
            ));
        }
        consumed_trace_rows += events.len();
        assert_eq!(row.get("geometry"), "precision");
        assert_eq!(row.get("scope"), "loaded");
        assert_eq!(row.count("query_nodes"), 1024);
        assert_eq!(
            row.filter(),
            SourceOwnerFilter {
                include_flags: 13,
                exclude_flags: 0
            }
        );
        assert_eq!(row.count("point_limit"), 74);
        for flag in ["optimize", "normalize", "ignore", "force"] {
            assert!(!row.flag(flag));
        }
        assert_eq!(row.hex("collision_width"), 0.125_f32.to_bits());
        let selected = owners[index]
            .select_for_calculate(SLOT, row.point("from"), row.point("to"))
            .unwrap();
        if row.count("step") == 0 {
            assert_eq!(row.get("oracle"), "compatibility-zero-v1");
            assert!(initial[index].is_none());
            let result = selected
                .loaded_cold_same_polygon_with_filter(
                    crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
                    row.filter(),
                    row.config().normalization,
                    SourceSamePolygonBudget::default(),
                    until(),
                )
                .unwrap_or_else(|error| panic!("source-origin cold {label}: {error:?}"));
            assert_eq!(result.check_query(&selected), Ok(()));
            compare_cold(row, &result, &events);
            initial[index] = Some(result);
        } else {
            assert_eq!(row.get("oracle"), "original-retained-defined");
            let cold = initial[index].as_ref().unwrap();
            let result = if let Some(prior) = previous[index].as_ref() {
                assert_eq!(row.refs("before_storage"), prior.storage());
                assert_eq!(row.count("before_active"), prior.active_len());
                assert_eq!(row.count("previous_type"), prior.path_type() as usize);
                let result = selected
                    .loaded_retained_straight(
                        prior,
                        row.config(),
                        row.hex("collision_width"),
                        SourceStraightPathBudget::default(),
                        until(),
                    )
                    .unwrap();
                assert_eq!(result.check_predecessor(prior), Ok(()));
                result
            } else {
                assert_eq!(row.refs("before_storage"), cold.compatibility().storage());
                assert_eq!(
                    row.count("before_active"),
                    cold.compatibility().active_len()
                );
                assert_eq!(row.count("previous_type"), cold.path_type() as usize);
                let result = selected
                    .loaded_retained_straight_from_same_polygon(
                        cold,
                        row.config(),
                        row.hex("collision_width"),
                        SourceStraightPathBudget::default(),
                        until(),
                    )
                    .unwrap();
                assert_eq!(result.check_same_polygon_predecessor(cold), Ok(()));
                result
            };
            assert_eq!(result.check_query(&selected), Ok(()));
            compare_straight(row, &result, cold, &events);
            previous[index] = Some(result);
        }
    }
    assert_eq!(
        consumed_trace_rows,
        traces.len(),
        "every original trace belongs to one consumed case"
    );
    for result in previous.iter().flatten() {
        assert_eq!(result.query_capacity(), 1024);
    }
}
