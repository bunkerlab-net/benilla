// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{collections::BTreeSet, ffi::c_void, time::Duration};
const BASE: u64 = 4503599627370496;
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn p(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn config() -> SourcePointPathConfig {
    SourcePointPathConfig {
        optimize_path: true,
        normalize_z: false,
        ignore_normalization: false,
        force_destination: false,
        previous_path_type: 0,
    }
}
fn mesh() -> Mesh {
    Mesh::load(
        include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture"),
        &[TileInput {
            grid: [31, 31],
            bytes: include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
        }],
    )
    .unwrap()
}
fn seed(mesh: &Mesh, refs: &[u64]) -> WarmCache {
    let mut storage = vec![0; 74];
    storage[..refs.len()].copy_from_slice(refs);
    WarmCache::authored_fixture(mesh, &storage, refs.len()).unwrap()
}
#[test]
fn same_and_cut_bypass_raycast_and_straight_ignores_optimizer() {
    let mut mesh = mesh();
    let before = seed(&mesh, &[BASE, BASE + 1]);
    let snapshot = before.clone();
    let budget = SourceStraightPathBudget {
        raycast_polygons: 0,
        ..Default::default()
    };
    let same = mesh
        .loaded_source_warm_straight_path(
            &before,
            p(5., 5., 0.),
            p(5., 6., 0.),
            50f32.to_bits(),
            SourcePointPathConfig {
                previous_path_type: 4,
                ..config()
            },
            budget,
            until(),
        )
        .unwrap();
    assert_eq!(same.phase(), SourceWarmPolyPhase::SamePolygon);
    assert_eq!(same.path_type(), 1);
    assert!(same.ray().is_none());
    assert_eq!(same.points(), &[p(5., 5., 0.), p(5., 6., 0.)]);
    let cut = mesh
        .loaded_source_warm_straight_path(
            &before,
            p(5., 5., 0.),
            p(5., 15., 0.),
            50f32.to_bits(),
            config(),
            budget,
            until(),
        )
        .unwrap();
    assert_eq!(cut.phase(), SourceWarmPolyPhase::Cut);
    assert!(cut.ray().is_none());
    assert_eq!(cut.points().len(), 4);
    let unoptimized = mesh
        .loaded_source_warm_straight_path(
            &before,
            p(5., 5., 0.),
            p(5., 15., 0.),
            50f32.to_bits(),
            SourcePointPathConfig {
                optimize_path: false,
                ..config()
            },
            budget,
            until(),
        )
        .unwrap();
    assert_eq!(cut.points(), unoptimized.points());
    assert_eq!(before, snapshot);
}
#[test]
fn clipped_hit_mutates_end_and_width_rejection_retains_empty_points_and_ray_cache() {
    let mut mesh = mesh();
    let before = seed(&mesh, &[BASE]);
    let snapshot = before.clone();
    let from = p(5., 5., 0.);
    let to = p(25., 5., 0.);
    let clipped = mesh
        .loaded_source_warm_straight_path(
            &before,
            from,
            to,
            0f32.to_bits(),
            config(),
            Default::default(),
            until(),
        )
        .unwrap();
    assert_eq!(clipped.path_type(), 4);
    assert_eq!(clipped.requested_end(), to);
    assert_eq!(clipped.end(), p(10., 5., 0.));
    assert_eq!(clipped.actual_end(), clipped.end());
    assert_eq!(clipped.points(), &[from, p(9., 5., 0.), p(10., 5., 0.)]);
    assert_eq!(clipped.ray().unwrap().hit_bits, 0.25f32.to_bits());
    let rejected = mesh
        .loaded_source_warm_straight_path(
            &before,
            from,
            to,
            0.2f32.to_bits(),
            config(),
            Default::default(),
            until(),
        )
        .unwrap();
    assert_eq!(rejected.path_type(), 8);
    assert!(rejected.points().is_empty());
    assert_eq!(rejected.end(), to);
    assert_eq!(rejected.actual_end(), to);
    assert_eq!(rejected.after().active_len(), 1);
    assert!(rejected.diagnostics().heights.is_empty());
    assert_eq!(before, snapshot);
    // The straight result carries a usable terminal cache. Prior INCOMPLETE remains meaningful.
    for (result, expected_type) in [(&clipped, 4), (&rejected, 1)] {
        let warm = mesh
            .loaded_source_warm_cache(
                result.after(),
                from,
                p(5., 15., 0.),
                Default::default(),
                until(),
            )
            .unwrap();
        let smooth = mesh
            .loaded_source_warm_point_path(
                &warm,
                SourcePointPathConfig {
                    previous_path_type: result.path_type(),
                    ..config()
                },
                Default::default(),
                until(),
            )
            .unwrap();
        assert_eq!(smooth.path_type(), expected_type);
    }
}
#[test]
fn identity_config_cold_and_budget_errors_discard_private_candidates() {
    let mut mesh = mesh();
    let before = seed(&mesh, &[BASE]);
    let snapshot = before.clone();
    let from = p(5., 5., 0.);
    let to = p(25., 5., 0.);
    let query = |mesh: &mut Mesh, budget, deadline| {
        mesh.loaded_source_warm_straight_path(
            &before,
            from,
            to,
            0f32.to_bits(),
            config(),
            budget,
            deadline,
        )
    };
    let result = query(&mut mesh, Default::default(), until()).unwrap();
    let d = result.diagnostics();
    let exact = SourceStraightPathBudget {
        lookup_primitives: result.trace().len(),
        raycast_polygons: d.raycast_polygons,
        sampled_points: d.sampled_points,
        native_primitives: d.native_primitives,
    };
    assert_eq!(query(&mut mesh, exact, until()).unwrap(), result);
    for budget in [
        SourceStraightPathBudget {
            lookup_primitives: exact.lookup_primitives - 1,
            ..exact
        },
        SourceStraightPathBudget {
            raycast_polygons: exact.raycast_polygons - 1,
            ..exact
        },
        SourceStraightPathBudget {
            sampled_points: exact.sampled_points - 1,
            ..exact
        },
        SourceStraightPathBudget {
            native_primitives: exact.native_primitives - 1,
            ..exact
        },
    ] {
        assert!(query(&mut mesh, budget, until()).is_err());
        assert_eq!(before, snapshot);
        assert!(query(&mut mesh, Default::default(), until()).is_ok());
    }
    assert_eq!(
        query(&mut mesh, Default::default(), Instant::now()).unwrap_err(),
        SourceStraightPathError::Boundary(Error::Deadline)
    );
    let mut other = self::mesh();
    assert_eq!(
        query(&mut other, Default::default(), until()).unwrap_err(),
        SourceStraightPathError::Boundary(Error::Identity)
    );
    for width in [f32::NAN, f32::INFINITY, -1.] {
        assert_eq!(
            mesh.loaded_source_warm_straight_path(
                &before,
                from,
                to,
                width.to_bits(),
                config(),
                Default::default(),
                until()
            )
            .unwrap_err(),
            SourceStraightPathError::InvalidCollisionWidth
        );
    }
    let cold = seed(&mesh, &[]);
    assert_eq!(
        mesh.loaded_source_warm_straight_path(
            &cold,
            from,
            to,
            0f32.to_bits(),
            config(),
            Default::default(),
            until()
        )
        .unwrap_err(),
        SourceStraightPathError::WarmCache(SourceWarmCacheError::ColdDistanceUndefined)
    );
    assert_eq!(before, snapshot);
}

// These rows are emitted by unchanged original PathFinder methods in an independent
// executable. Raw helper rows never establish a loaded straight-calculation authority.
struct OracleRow<'a> {
    headings: Vec<&'a str>,
    values: Vec<&'a str>,
}
impl OracleRow<'_> {
    fn get(&self, name: &str) -> &str {
        self.values[self
            .headings
            .iter()
            .position(|field| *field == name)
            .unwrap_or_else(|| panic!("missing straight source field {name}"))]
    }
    fn count(&self, name: &str) -> usize {
        self.get(name).parse().unwrap()
    }
    fn reference(&self, name: &str) -> u64 {
        self.get(name).parse().unwrap()
    }
    fn hex(&self, name: &str) -> u32 {
        u32::from_str_radix(self.get(name), 16).unwrap()
    }
    fn optional_hex(&self, name: &str) -> Option<u32> {
        (self.get(name) != "-").then(|| self.hex(name))
    }
    fn flag(&self, name: &str) -> bool {
        match self.get(name) {
            "0" => false,
            "1" => true,
            other => panic!("invalid source flag {name}: {other}"),
        }
    }
    fn point(&self, name: &str) -> [u32; 3] {
        parse_point(self.get(name))
    }
    fn points(&self, name: &str) -> Vec<[u32; 3]> {
        match self.get(name) {
            "-" => Vec::new(),
            values => values.split(';').map(parse_point).collect(),
        }
    }
    fn refs(&self, name: &str) -> Vec<u64> {
        match self.get(name) {
            "-" => Vec::new(),
            values => values
                .split(',')
                .map(|value| value.parse().unwrap())
                .collect(),
        }
    }
    fn config(&self) -> SourcePointPathConfig {
        SourcePointPathConfig {
            optimize_path: self.flag("optimize"),
            normalize_z: self.flag("normalize"),
            ignore_normalization: self.flag("ignore"),
            force_destination: self.flag("force"),
            previous_path_type: self.count("previous_type") as u32,
        }
    }
    fn phase(&self) -> SourceWarmPolyPhase {
        match self.get("phase") {
            "same" => SourceWarmPolyPhase::SamePolygon,
            "cut" => SourceWarmPolyPhase::Cut,
            "replacement" => SourceWarmPolyPhase::Replacement,
            other => panic!("not a loaded source phase: {other}"),
        }
    }
}
fn parse_point(value: &str) -> [u32; 3] {
    value
        .split(',')
        .map(|bits| u32::from_str_radix(bits, 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}
fn oracle_rows(fixture: &'static str) -> Vec<OracleRow<'static>> {
    let headings: Vec<_> = fixture
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .expect("missing original straight-path schema");
    fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            assert_eq!(headings.len(), values.len(), "source row: {line}");
            OracleRow {
                headings: headings.clone(),
                values,
            }
        })
        .collect()
}
fn cases() -> Vec<OracleRow<'static>> {
    oracle_rows(include_str!(
        "../tools/fixtures/straight-path/cases.test-fixture"
    ))
}
fn sequences() -> Vec<OracleRow<'static>> {
    oracle_rows(include_str!(
        "../tools/fixtures/straight-path/sequences.test-fixture"
    ))
}
fn traces(label: &str) -> Vec<OracleRow<'static>> {
    let rows: Vec<_> = oracle_rows(include_str!(
        "../tools/fixtures/straight-path/traces.test-fixture"
    ))
    .into_iter()
    .filter(|row| row.get("label").rsplit_once('_').unwrap().0 == label)
    .collect();
    for (ordinal, row) in rows.iter().enumerate() {
        assert_eq!(row.count("ordinal"), ordinal, "trace order {label}");
    }
    rows
}
fn oracle_mesh(geometry: &str) -> Mesh {
    let (params, grid, bytes): (&[u8], [u8; 2], &[u8]) = match geometry {
        "precision" => return mesh(),
        "ground" => (
            include_bytes!("../tools/fixtures/smoothing/params.test-fixture"),
            [32, 32],
            include_bytes!("../tools/fixtures/smoothing/ground.test-fixture"),
        ),
        "thin" => (
            include_bytes!("../tools/fixtures/smoothing/params.test-fixture"),
            [32, 32],
            include_bytes!("../tools/fixtures/smoothing/thin.test-fixture"),
        ),
        "long600" => (
            include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture"),
            [32, 32],
            include_bytes!("../tools/fixtures/direct-corridor/long600.test-fixture"),
        ),
        other => panic!("unknown authored straight geometry {other}"),
    };
    Mesh::load(params, &[TileInput { grid, bytes }]).unwrap()
}
fn oracle_seed(mesh: &Mesh, row: &OracleRow<'_>) -> WarmCache {
    WarmCache::authored_fixture(
        mesh,
        &row.refs("before_storage"),
        row.count("before_active"),
    )
    .unwrap()
}
fn compare_selection(
    row: &OracleRow<'_>,
    before: &WarmCache,
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
    // Before the transition the source lookups can only append, never compact storage.
    let mut lookup_storage = before.storage().to_vec();
    for selected in [start, end] {
        if selected.appended {
            assert!(matches!(
                selected.origin,
                SourceWarmLookupOrigin::Nearest5 | SourceWarmLookupOrigin::Nearest10
            ));
            lookup_storage.push(selected.polygon);
        }
    }
    assert_eq!(
        lookup_storage,
        row.refs("lookup_storage"),
        "entire lookup storage {label}"
    );
}
fn compare_cache(row: &OracleRow<'_>, after: &WarmCache) {
    let label = row.get("label");
    assert_eq!(
        after.storage(),
        row.refs("after_storage"),
        "entire terminal storage {label}"
    );
    assert_eq!(
        after.active_len(),
        row.count("after_active"),
        "terminal active {label}"
    );
    assert_eq!(
        after.filter(),
        SourceOwnerFilter {
            include_flags: 1,
            exclude_flags: 0
        },
        "filter {label}"
    );
    assert_eq!(after.point_limit(), 74, "source point limit {label}");
}
fn compare_lookup_trace(label: &str, actual: &[SourceWarmTrace], source: &[OracleRow<'_>]) {
    let actual: Vec<_> = actual
        .iter()
        .filter(|event| {
            matches!(
                event.primitive,
                SourceWarmPrimitive::Active
                    | SourceWarmPrimitive::Tail
                    | SourceWarmPrimitive::Nearest5
                    | SourceWarmPrimitive::Nearest10
            )
        })
        .collect();
    let source: Vec<_> = source
        .iter()
        .filter(|event| matches!(event.get("primitive"), "closest" | "nearest5" | "nearest10"))
        .collect();
    assert_eq!(actual.len(), source.len(), "lookup primitive count {label}");
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
        assert_eq!(source.reference("target"), 0, "lookup target {name}");
        assert_eq!(source.count("count"), 0, "lookup count {name}");
        assert!(source.refs("refs").is_empty(), "lookup refs {name}");
        assert_eq!(source.hex("scalar"), 0, "lookup scalar {name}");
        let input = source.point("input");
        let key = (actual.endpoint, actual.primitive, input);
        if previous != Some(key) {
            minimum = f32::MAX;
        }
        previous = Some(key);
        let output = source.points("output");
        let distance = if actual.status & SUCCESS != 0 && !output.is_empty() {
            assert_eq!(output.len(), 1, "closest output count {name}");
            let a = input.map(f32::from_bits);
            let b = output[0].map(f32::from_bits);
            // Detour sums Y, Z, X in that order; reassociation changes exact bits.
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
fn compare_heights(label: &str, actual: &[SourceStraightHeight], source: &[OracleRow<'_>]) {
    let source: Vec<_> = source
        .iter()
        .filter(|event| event.get("primitive") == "height")
        .collect();
    assert_eq!(actual.len(), source.len(), "height count {label}");
    for (actual, source) in actual.iter().zip(source) {
        let name = source.get("label");
        assert_eq!(actual.polygon, source.reference("ref"), "height ref {name}");
        assert_eq!(actual.status, source.hex("status"), "height status {name}");
        assert_eq!(
            actual.before,
            source.point("input"),
            "height initialized input {name}"
        );
        assert_eq!(actual.after, source.point("output"), "height output {name}");
        assert_eq!(
            actual.after[2],
            source.hex("scalar"),
            "height scalar {name}"
        );
        if actual.status & (1 << 31) != 0 {
            assert_eq!(
                actual.before[2], actual.after[2],
                "ignored failure preserves initialized Z {name}"
            );
        }
    }
}
fn run_loaded(mesh: &mut Mesh, row: &OracleRow<'_>) -> LoadedSourceWarmStraightPath {
    assert_eq!(row.get("scope"), "loaded");
    let label = row.get("label");
    let before = oracle_seed(mesh, row);
    let snapshot = before.clone();
    let result = mesh
        .loaded_source_warm_straight_path(
            &before,
            row.point("from"),
            row.point("to"),
            row.hex("collision_width"),
            row.config(),
            Default::default(),
            until(),
        )
        .unwrap_or_else(|error| panic!("source straight {label}: {error:?}"));
    assert_eq!(before, snapshot, "private candidate changed input {label}");
    assert_eq!(result.from(), row.point("from"), "start {label}");
    assert_eq!(
        result.requested_end(),
        row.point("to"),
        "requested end {label}"
    );
    assert_eq!(
        result.collision_width_bits(),
        row.hex("collision_width"),
        "width {label}"
    );
    assert_eq!(result.config(), row.config(), "config {label}");
    compare_selection(
        row,
        &before,
        result.start_lookup(),
        result.end_lookup(),
        result.phase(),
    );
    compare_cache(row, result.after());
    assert_eq!(
        result.path_type(),
        row.count("path_type") as u32,
        "terminal type {label}"
    );
    assert_eq!(result.end(), row.point("end"), "end {label}");
    assert_eq!(
        result.actual_end(),
        row.point("actual_end"),
        "actual end {label}"
    );
    assert_eq!(
        result.points(),
        row.points("points"),
        "all point bits {label}"
    );
    let trace = traces(label);
    compare_lookup_trace(label, result.trace(), &trace);
    compare_heights(label, &result.diagnostics().heights, &trace);
    assert_eq!(
        result.diagnostics().heights.len(),
        row.count("height_calls"),
        "height calls {label}"
    );
    assert_eq!(
        result
            .diagnostics()
            .heights
            .iter()
            .filter(|h| h.status & (1 << 31) != 0)
            .count(),
        row.count("height_failures"),
        "height failures {label}"
    );
    match result.ray() {
        None => {
            assert_eq!(row.get("ray_status"), "-", "ray absent {label}");
            assert_eq!(row.count("width_calls"), 0, "width bypass {label}");
            assert_eq!(
                result.diagnostics().raycast_polygons,
                0,
                "ray work bypass {label}"
            );
            assert_ne!(
                result.phase(),
                SourceWarmPolyPhase::Replacement,
                "ray phase {label}"
            );
        }
        Some(ray) => {
            assert_eq!(ray.status, row.hex("ray_status"), "ray status {label}");
            assert_eq!(ray.hit_bits, row.hex("ray_hit"), "ray hit {label}");
            let normal = row.point("ray_normal");
            assert_eq!(
                ray.normal_bits,
                [normal[1], normal[2], normal[0]],
                "Detour normal YZX {label}"
            );
            assert_eq!(
                ray.polygons.len(),
                row.count("ray_active"),
                "ray active {label}"
            );
            assert_eq!(
                ray.adjusted_hit_bits,
                row.optional_hex("adjusted_hit"),
                "width subtraction {label}"
            );
            assert_eq!(
                usize::from(ray.adjusted_hit_bits.is_some()),
                row.count("width_calls"),
                "width calls {label}"
            );
            let mut storage = row.refs("lookup_storage");
            storage[..ray.polygons.len()].copy_from_slice(&ray.polygons);
            assert_eq!(
                storage,
                row.refs("ray_storage"),
                "entire ray storage {label}"
            );
            let rays: Vec<_> = trace
                .iter()
                .filter(|event| event.get("primitive") == "raycast")
                .collect();
            assert_eq!(rays.len(), 1, "ray trace count {label}");
            assert_eq!(rays[0].refs("refs"), ray.polygons, "ray trace refs {label}");
            assert_eq!(
                rays[0].points("input"),
                [result.from(), result.requested_end()],
                "ray inputs {label}"
            );
            assert_eq!(
                rays[0].hex("status"),
                ray.status,
                "ray trace status {label}"
            );
            assert_eq!(
                rays[0].hex("scalar"),
                ray.hit_bits,
                "ray trace scalar {label}"
            );
            assert_eq!(rays[0].point("output"), normal, "ray trace normal {label}");
        }
    }
    let clipped_height = result
        .ray()
        .is_some_and(|ray| ray.hit_bits != f32::MAX.to_bits());
    for (index, height) in result.diagnostics().heights.iter().enumerate() {
        assert_eq!(
            height.endpoint,
            if clipped_height && index == 0 { 3 } else { 4 },
            "height role {label}"
        );
    }
    result
}
#[test]
fn original_loaded_straight_rows_match_all_bits_retained_storage_and_native_traces() {
    let mut labels = BTreeSet::new();
    let mut phases = BTreeSet::new();
    let mut loaded = 0;
    for row in cases() {
        assert!(
            labels.insert(row.get("label").to_owned()),
            "duplicate straight source label"
        );
        match row.get("scope") {
            "helper" => continue,
            "loaded" => (),
            scope => panic!("unknown source scope {scope}"),
        }
        loaded += 1;
        phases.insert(row.get("phase").to_owned());
        let mut mesh = oracle_mesh(row.get("geometry"));
        let result = run_loaded(&mut mesh, &row);
        if row.get("label") == "ray_buffer_no_hit_sampling_appends" {
            let ray = result.ray().unwrap();
            assert_eq!(ray.status, SUCCESS | (1 << 4));
            assert_eq!(ray.polygons.len(), 74);
            assert!(result.diagnostics().raycast_polygons > 74);
            assert!(result.after().storage().len() > row.refs("ray_storage").len());
            assert_eq!(result.path_type(), 4);
        }
    }
    assert_eq!(loaded, 24);
    assert_eq!(labels.len(), 35);
    assert_eq!(
        phases,
        BTreeSet::from([
            "same".to_owned(),
            "cut".to_owned(),
            "replacement".to_owned()
        ])
    );
}
#[test]
fn original_same_object_straight_to_smooth_sequences_keep_terminal_cache_and_prior_type() {
    let cases = cases();
    let sequences = sequences();
    assert_eq!(sequences.len(), 5);
    for row in sequences {
        let label = row.get("label");
        let first = cases
            .iter()
            .find(|first| first.get("label") == row.get("first_case"))
            .unwrap();
        let mut mesh = oracle_mesh(first.get("geometry"));
        let straight = run_loaded(&mut mesh, first);
        assert_ne!(
            straight.path_type() & (4 | 8),
            0,
            "source fallback predicate {label}"
        );
        assert_eq!(
            straight.after().storage(),
            row.refs("before_storage"),
            "same cache fallback {label}"
        );
        assert_eq!(
            straight.after().active_len(),
            row.count("before_active"),
            "same active fallback {label}"
        );
        assert_eq!(
            straight.path_type(),
            row.count("previous_type") as u32,
            "same type fallback {label}"
        );
        let before = straight.after().clone();
        let warm = mesh
            .loaded_source_warm_cache(
                straight.after(),
                row.point("from"),
                row.point("to"),
                Default::default(),
                until(),
            )
            .unwrap_or_else(|error| panic!("fallback selection {label}: {error:?}"));
        assert_eq!(
            straight.after(),
            &before,
            "fallback changed straight result {label}"
        );
        compare_selection(&row, &before, warm.start(), warm.end(), warm.phase());
        assert_eq!(
            warm.direct_status(),
            row.optional_hex("direct_status"),
            "fallback direct status {label}"
        );
        compare_lookup_trace(label, warm.trace(), &traces(label));
        let snapshot = warm.try_copy().unwrap();
        let smooth = mesh
            .loaded_source_warm_point_path(
                &warm,
                SourcePointPathConfig {
                    optimize_path: row.flag("optimize"),
                    previous_path_type: straight.path_type(),
                    force_destination: false,
                    ..config()
                },
                Default::default(),
                until(),
            )
            .unwrap_or_else(|error| panic!("fallback smooth {label}: {error:?}"));
        assert_eq!(warm, snapshot, "fallback changed selected cache {label}");
        assert_eq!(
            smooth.warm(),
            &warm,
            "fallback retained warm lineage {label}"
        );
        compare_cache(&row, smooth.after());
        assert_eq!(
            smooth.smooth_status(),
            row.optional_hex("smooth_status"),
            "fallback smooth status {label}"
        );
        assert_eq!(
            smooth.smooth_points(),
            row.points("smooth_points"),
            "fallback raw smooth bits {label}"
        );
        assert_eq!(
            smooth.points(),
            row.points("points"),
            "fallback final bits {label}"
        );
        assert_eq!(warm.to(), row.point("end"), "fallback end {label}");
        assert_eq!(
            smooth.actual_end(),
            row.point("actual_end"),
            "fallback actual end {label}"
        );
        assert_eq!(
            smooth.path_type(),
            row.count("path_type") as u32,
            "fallback type {label}"
        );
    }
}

unsafe extern "C" {
    fn benilla_source_straight_plan(
        from: *const f32,
        to: *const f32,
        max_points: u32,
        steps: *mut u32,
        diff: *mut f32,
    ) -> u32;
    fn benilla_source_straight_advance(previous: *mut f32, diff: *const f32);
    fn benilla_nav_straight_height(
        mesh: *mut c_void,
        polygon: u64,
        point: *const f32,
        height: *mut f32,
    ) -> u32;
    fn benilla_source_straight_terminal(
        from: *const f32,
        end: *const f32,
        count: u32,
        path_type: u32,
        force: u32,
        last: *mut f32,
        actual: *mut f32,
        shortcut: *mut u32,
    ) -> u32;
}
#[test]
fn original_raw_straight_helper_rows_keep_threshold_rounding_and_equality_only_limit() {
    use crate::warm_cache::native as warm;
    let mut count = 0;
    let mut raw_counts = BTreeSet::new();
    for row in cases()
        .into_iter()
        .filter(|row| row.get("scope") == "helper")
    {
        count += 1;
        let label = row.get("label");
        assert_eq!(row.get("phase"), "sample");
        assert_eq!(
            row.reference("start_ref"),
            0,
            "helper has no selected start {label}"
        );
        assert_eq!(
            row.reference("end_ref"),
            0,
            "helper has no selected end {label}"
        );
        assert_eq!(row.get("ray_status"), "-", "helper has no ray {label}");
        let mut mesh = oracle_mesh(row.get("geometry"));
        let before = oracle_seed(&mesh, &row);
        let mut candidate = before.clone();
        assert_eq!(
            candidate.storage(),
            row.refs("lookup_storage"),
            "helper initial storage {label}"
        );
        // Obtain a bounded native lookup work context. Its initial selected proposal is
        // kept separate; only the authored helper cache feeds the raw sample helper.
        let (mut context, mut work) = warm::prepare(
            &mut mesh,
            &before,
            row.point("from"),
            row.point("from"),
            Default::default(),
            until(),
        )
        .unwrap();
        let from = row.point("from").map(f32::from_bits);
        let end = row.point("to").map(f32::from_bits);
        let mut steps = 0;
        let mut diff = [0.; 3];
        // SAFETY: finite fixed arrays and initialized bounded outputs, no retained pointers.
        let plan = unsafe {
            benilla_source_straight_plan(
                from.as_ptr(),
                end.as_ptr(),
                SOURCE_STRAIGHT_PATH_MAX_POINTS as u32,
                &mut steps,
                diff.as_mut_ptr(),
            )
        };
        assert!(matches!(plan, 1 | 2), "raw plan {label}: {plan}");
        let mut points = Vec::new();
        let mut heights = Vec::new();
        if plan == 1 {
            points.push(row.point("from"));
            let mut previous = from;
            for _ in 0..steps {
                // SAFETY: initialized finite arrays produced by the bounded source plan.
                unsafe { benilla_source_straight_advance(previous.as_mut_ptr(), diff.as_ptr()) };
                let input = previous.map(f32::to_bits);
                let converted = detour(input).unwrap();
                match warm::lookup(&mesh, &mut candidate, &converted, 4, &mut work) {
                    Ok(found) => {
                        let mut z = converted[1];
                        // SAFETY: live mesh, native-selected ref, finite input and initialized Z.
                        let status = unsafe {
                            benilla_nav_straight_height(
                                mesh.raw.as_ptr(),
                                found.polygon,
                                converted.as_ptr(),
                                &mut z,
                            )
                        };
                        previous[2] = z;
                        heights.push(SourceStraightHeight {
                            endpoint: 4,
                            polygon: found.polygon,
                            before: input,
                            after: previous.map(f32::to_bits),
                            status,
                        });
                    }
                    Err(SourceWarmCacheError::MissingPolygon) => (),
                    Err(error) => panic!("raw sample lookup {label}: {error:?}"),
                }
                points.push(previous.map(f32::to_bits));
            }
            points.push(row.point("to"));
        }
        let raw_count = points.len();
        if label.starts_with("sample_count_") {
            raw_counts.insert(raw_count);
            assert_eq!(
                raw_count,
                label
                    .strip_prefix("sample_count_")
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
            );
        }
        let mut actual = end;
        let mut last = points
            .last()
            .copied()
            .unwrap_or(row.point("to"))
            .map(f32::from_bits);
        let mut shortcut = 0;
        // SAFETY: bounded raw helper count and initialized fixed arrays, no authority minted.
        let path_type = unsafe {
            benilla_source_straight_terminal(
                from.as_ptr(),
                end.as_ptr(),
                raw_count as u32,
                row.count("previous_type") as u32,
                u32::from(row.flag("force")),
                last.as_mut_ptr(),
                actual.as_mut_ptr(),
                &mut shortcut,
            )
        };
        if shortcut == 1 {
            candidate.clear_active();
            points = vec![row.point("from"), actual.map(f32::to_bits)];
        } else if let Some(point) = points.last_mut() {
            *point = last.map(f32::to_bits);
        }
        assert_eq!(
            path_type,
            row.count("path_type") as u32,
            "raw helper terminal type {label}"
        );
        assert_eq!(
            points,
            row.points("points"),
            "raw helper point bits {label}"
        );
        assert_eq!(
            actual.map(f32::to_bits),
            row.point("actual_end"),
            "raw helper actual end {label}"
        );
        assert_eq!(
            end.map(f32::to_bits),
            row.point("end"),
            "raw helper end {label}"
        );
        compare_cache(&row, &candidate);
        warm::finish_trace(&mut context, work);
        let sample_trace: Vec<_> = context
            .trace()
            .iter()
            .copied()
            .filter(|event| event.endpoint == 4)
            .collect();
        let source = traces(label);
        compare_lookup_trace(label, &sample_trace, &source);
        compare_heights(label, &heights, &source);
        assert_eq!(
            heights.len(),
            row.count("height_calls"),
            "raw helper height calls {label}"
        );
        assert_eq!(
            heights
                .iter()
                .filter(|height| height.status & (1 << 31) != 0)
                .count(),
            row.count("height_failures"),
            "raw helper height failures {label}"
        );
        assert_eq!(
            before.storage(),
            row.refs("before_storage"),
            "raw helper immutable seed {label}"
        );
    }
    assert_eq!(count, 11);
    assert_eq!(raw_counts, BTreeSet::from([73, 74, 75]));
}
#[test]
fn original_excluded_domains_are_rejected_without_cold_repair_or_mutating_input() {
    let domains = oracle_rows(include_str!(
        "../tools/fixtures/straight-path/domains.test-fixture"
    ));
    assert_eq!(domains.len(), 10);
    let mut mesh = mesh();
    for row in domains {
        let label = row.get("label");
        assert_eq!(
            row.count("invoked"),
            0,
            "unadmitted original was invoked {label}"
        );
        let mut storage = vec![0; 74];
        storage[0] = BASE;
        let mut active = 1;
        let mut from = p(5., 5., 0.);
        let mut to = p(5., 15., 0.);
        let mut width = 0f32.to_bits();
        let mut config = config();
        let (classification, expected) = match label {
            "invalid_retained_reference" => {
                storage.push(1);
                assert_eq!(row.get("classification"), "invalid_reference");
                assert_eq!(
                    WarmCache::authored_fixture(&mesh, &storage, active).unwrap_err(),
                    Error::Identity
                );
                continue;
            }
            "active_over_capacity" => {
                assert_eq!(row.get("classification"), "active_bounds");
                assert_eq!(
                    WarmCache::authored_fixture(&mesh, &storage, 75).unwrap_err(),
                    Error::Format
                );
                continue;
            }
            "cold_no_tail" => {
                active = 0;
                storage[0] = 0;
                (
                    "cold_uninitialized",
                    SourceStraightPathError::WarmCache(SourceWarmCacheError::ColdDistanceUndefined),
                )
            }
            "active_zero_reference" => {
                storage[0] = 0;
                (
                    "invalid_reference",
                    SourceStraightPathError::Boundary(Error::Format),
                )
            }
            "nonfinite_endpoint" => {
                from[0] = f32::INFINITY.to_bits();
                (
                    "coordinate_domain",
                    SourceStraightPathError::WarmCache(SourceWarmCacheError::Boundary(
                        Error::Format,
                    )),
                )
            }
            "nan_width" => {
                width = f32::NAN.to_bits();
                (
                    "width_domain",
                    SourceStraightPathError::InvalidCollisionWidth,
                )
            }
            "negative_width" => {
                width = (-1f32).to_bits();
                (
                    "width_domain",
                    SourceStraightPathError::InvalidCollisionWidth,
                )
            }
            "active_normalization" => {
                config.normalize_z = true;
                (
                    "normalization_unsupported",
                    SourceStraightPathError::ActiveNormalizationUnsupported,
                )
            }
            "invalid_type" => {
                config.previous_path_type = 64;
                (
                    "type_domain",
                    SourceStraightPathError::InvalidPreviousPathType,
                )
            }
            "sample_operational_bound" => {
                to = p(5000., 15., 0.);
                (
                    "sample_bound",
                    SourceStraightPathError::WarmCache(SourceWarmCacheError::Boundary(
                        Error::Budget,
                    )),
                )
            }
            other => panic!("unconsumed original domain {other}"),
        };
        assert_eq!(
            row.get("classification"),
            classification,
            "domain classification {label}"
        );
        let before = WarmCache::authored_fixture(&mesh, &storage, active).unwrap();
        let snapshot = before.clone();
        assert_eq!(
            mesh.loaded_source_warm_straight_path(
                &before,
                from,
                to,
                width,
                config,
                Default::default(),
                until()
            )
            .unwrap_err(),
            expected,
            "domain rejection {label}"
        );
        assert_eq!(
            before, snapshot,
            "domain rejected private candidate {label}"
        );
    }
}
#[test]
fn every_loaded_source_case_has_exact_work_budgets_and_discards_partial_candidates() {
    for row in cases()
        .into_iter()
        .filter(|row| row.get("scope") == "loaded")
    {
        let label = row.get("label");
        let mut mesh = oracle_mesh(row.get("geometry"));
        let before = oracle_seed(&mesh, &row);
        let snapshot = before.clone();
        let query = |mesh: &mut Mesh, budget| {
            mesh.loaded_source_warm_straight_path(
                &before,
                row.point("from"),
                row.point("to"),
                row.hex("collision_width"),
                row.config(),
                budget,
                until(),
            )
        };
        let result = query(&mut mesh, Default::default()).unwrap();
        let d = result.diagnostics();
        let exact = SourceStraightPathBudget {
            lookup_primitives: result.trace().len(),
            raycast_polygons: d.raycast_polygons,
            sampled_points: d.sampled_points,
            native_primitives: d.native_primitives,
        };
        assert_eq!(
            query(&mut mesh, exact).unwrap(),
            result,
            "exact budget {label}"
        );
        for field in 0..4 {
            let mut short = exact;
            let remaining = match field {
                0 => &mut short.lookup_primitives,
                1 => &mut short.raycast_polygons,
                2 => &mut short.sampled_points,
                3 => &mut short.native_primitives,
                _ => unreachable!(),
            };
            if *remaining == 0 {
                continue;
            }
            *remaining -= 1;
            assert!(
                matches!(
                    query(&mut mesh, short).unwrap_err(),
                    SourceStraightPathError::Boundary(Error::Budget)
                        | SourceStraightPathError::WarmCache(SourceWarmCacheError::Boundary(
                            Error::Budget
                        ))
                ),
                "one-short budget field {field}: {label}"
            );
            assert_eq!(before, snapshot, "budget preserved full input {label}");
            assert_eq!(
                query(&mut mesh, exact).unwrap(),
                result,
                "retry after partial candidate {label}"
            );
        }
    }
}
#[test]
fn operational_caps_are_separate_from_the_original_equality_only_point_limit() {
    assert_eq!(SOURCE_STRAIGHT_PATH_MAX_POINTS, 1026);
    assert_eq!(SOURCE_STRAIGHT_PATH_MAX_RAY_POLYGONS, 4096);
    let mut mesh = mesh();
    let before = seed(&mesh, &[BASE]);
    let snapshot = before.clone();
    for field in 0..4 {
        let mut budget = SourceStraightPathBudget::default();
        match field {
            0 => budget.lookup_primitives += 1,
            1 => budget.raycast_polygons += 1,
            2 => budget.sampled_points += 1,
            3 => budget.native_primitives += 1,
            _ => unreachable!(),
        }
        assert_eq!(
            mesh.loaded_source_warm_straight_path(
                &before,
                p(5., 5., 0.),
                p(25., 5., 0.),
                0f32.to_bits(),
                config(),
                budget,
                until()
            )
            .unwrap_err(),
            SourceStraightPathError::Boundary(Error::Budget)
        );
        assert_eq!(before, snapshot);
    }
    let origin = [0.; 3];
    let mut steps = 0;
    let mut diff = [0.; 3];
    let mut plan = |end: [f32; 3], cap| {
        // SAFETY: initialized finite arrays and fixed writable output buffers.
        unsafe {
            benilla_source_straight_plan(
                origin.as_ptr(),
                end.as_ptr(),
                cap,
                &mut steps,
                diff.as_mut_ptr(),
            )
        }
    };
    assert_eq!(plan([4096., 0., 0.], 1026), 1);
    assert_eq!(
        plan([f32::from_bits(4096f32.to_bits() + 1), 0., 0.], 1026),
        0
    );
    assert_eq!(plan([4096., 0., 0.], 1025), 1);
    assert_eq!(plan([4096., 0., 0.], 1024), 0);
    assert_eq!(plan([4096., 0., 0.], 1027), 0);
    assert_eq!(plan([4., 0., 0.], 1), 0);
    assert_eq!(plan([f32::INFINITY, 0., 0.], 1026), 0);
    assert_eq!(plan([f32::NAN, 0., 0.], 1026), 0);
    assert!(steps as usize + 2 <= SOURCE_STRAIGHT_PATH_MAX_POINTS);
}
