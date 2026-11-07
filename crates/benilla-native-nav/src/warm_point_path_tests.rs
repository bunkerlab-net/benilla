// SPDX-License-Identifier: GPL-3.0-or-later
//! Consumers of independently compiled, unchanged original PathFinder observations.
//! Every composed result starts with actual loaded warm selection. Authored cache inputs
//! are test-only fixtures validated against that mesh, never a production cache producer.
use super::*;
use std::{collections::BTreeSet, time::Duration};

const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const LEFT: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
const RIGHT: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
const GROUND: SourceOwnerFilter = SourceOwnerFilter {
    include_flags: 1,
    exclude_flags: 0,
};
const FAILURE: u32 = 1 << 31;

fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
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

fn oracle_mesh(geometry: &str) -> Mesh {
    let (params, grid, bytes): (&[u8], [u8; 2], &[u8]) = match geometry {
        "existing" => return existing_mesh(),
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
        "precision" => (
            include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture"),
            [31, 31],
            include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
        ),
        other => panic!("unknown authored point-path geometry {other}"),
    };
    Mesh::load(params, &[TileInput { grid, bytes }]).unwrap()
}

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
            .unwrap_or_else(|| panic!("missing original point-path field {name}"))]
    }
    fn hex(&self, name: &str) -> u32 {
        u32::from_str_radix(self.get(name), 16).unwrap()
    }
    fn count(&self, name: &str) -> usize {
        self.get(name).parse().unwrap()
    }
    fn flag(&self, name: &str) -> bool {
        match self.get(name) {
            "0" => false,
            "1" => true,
            value => panic!("invalid original flag {name}: {value}"),
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
            refs => refs
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
}

fn parse_point(text: &str) -> [u32; 3] {
    text.split(',')
        .map(|value| u32::from_str_radix(value, 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn oracle_rows(fixture: &'static str) -> Vec<OracleRow<'static>> {
    let headings: Vec<_> = fixture
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .expect("missing original point-path oracle schema");
    fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            assert_eq!(
                values.len(),
                headings.len(),
                "point-path oracle row: {line}"
            );
            OracleRow {
                headings: headings.clone(),
                values,
            }
        })
        .collect()
}

fn cases() -> Vec<OracleRow<'static>> {
    oracle_rows(include_str!(
        "../tools/fixtures/point-path/cases.test-fixture"
    ))
}

fn raw_cases() -> Vec<OracleRow<'static>> {
    oracle_rows(include_str!(
        "../tools/fixtures/point-path/postprocess.test-fixture"
    ))
}

fn selected(mesh: &mut Mesh, row: &OracleRow<'_>) -> LoadedSourceWarmCache {
    let before = WarmCache::authored_fixture(
        mesh,
        &row.refs("before_storage"),
        row.count("before_active"),
    )
    .unwrap();
    let snapshot = before.clone();
    let warm = mesh
        .loaded_source_warm_cache(
            &before,
            row.point("from"),
            row.point("to"),
            SourceWarmCacheBudget::default(),
            until(),
        )
        .unwrap_or_else(|error| panic!("{} warm selection: {error:?}", row.get("label")));
    assert_eq!(before, snapshot, "{} input cache", row.get("label"));
    assert_eq!(
        warm.after().storage(),
        row.refs("selected_storage"),
        "{} entire selected storage",
        row.get("label")
    );
    assert_eq!(warm.after().active_len(), row.count("selected_active"));
    assert_eq!(
        warm.phase(),
        match row.get("phase") {
            "same" => SourceWarmPolyPhase::SamePolygon,
            "cut" => SourceWarmPolyPhase::Cut,
            "replacement" => SourceWarmPolyPhase::Replacement,
            other => panic!("unknown loaded source phase {other}"),
        },
        "{} selected phase",
        row.get("label")
    );
    warm
}

fn original_zero(points: &[[u32; 3]]) -> bool {
    points.len() > 2
        && points
            .iter()
            .any(|point| point.iter().all(|bits| f32::from_bits(*bits) == 0.))
}

#[test]
fn original_loaded_point_paths_match_bits_status_type_end_and_all_retained_cache_slots() {
    let mut labels = BTreeSet::new();
    let mut phases = BTreeSet::new();
    let mut types = BTreeSet::new();
    let mut accepted = 0;
    let mut optimized = 0;
    let mut preserved_incomplete = 0;
    let mut failed_limit = 0;
    let mut early_stop = 0;
    let mut tails = 0;
    for row in cases() {
        let label = row.get("label");
        assert!(
            labels.insert(label.to_owned()),
            "duplicate source case {label}"
        );
        let mut mesh = oracle_mesh(row.get("geometry"));
        let warm = selected(&mut mesh, &row);
        let prior = warm.try_copy().unwrap();
        let config = row.config();
        let result = mesh.loaded_source_warm_point_path(
            &warm,
            config,
            SourcePointPathBudget::default(),
            until(),
        );
        assert_eq!(warm, prior, "point path mutated selected input {label}");
        if config.optimize_path && original_zero(&row.points("smooth_points")) {
            assert_eq!(
                result.unwrap_err(),
                SourcePointPathError::OriginalZeroPointOptimizationUnsupported,
                "{label}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        assert_eq!(result.warm(), &warm, "retained selection lineage {label}");
        assert_eq!(result.config(), config, "{label}");
        assert_eq!(
            result.points(),
            row.points("points"),
            "final XYZ bits {label}"
        );
        assert_eq!(result.actual_end(), row.point("actual_end"), "{label}");
        assert_eq!(result.path_type(), row.count("path_type") as u32, "{label}");
        assert_eq!(
            result.smooth_points(),
            row.points("smooth_points"),
            "{label}"
        );
        assert_eq!(
            result.smooth_status(),
            (row.get("smooth_status") != "-").then(|| row.hex("smooth_status")),
            "raw status {label}"
        );
        assert_eq!(
            result.after().storage(),
            row.refs("after_storage"),
            "entire terminal storage {label}"
        );
        assert_eq!(
            result.after().active_len(),
            row.count("after_active"),
            "{label}"
        );
        assert_eq!(result.after().filter(), GROUND, "{label}");
        assert_eq!(result.after().point_limit(), 74, "{label}");
        if warm.phase() == SourceWarmPolyPhase::SamePolygon {
            assert!(
                result.smooth_points().is_empty(),
                "same-polygon smoother ran {label}"
            );
            assert_eq!(result.smooth_stop(), None, "{label}");
            assert_eq!(
                result.diagnostics().smoothing,
                SourceSmoothingDiagnostics::default(),
                "{label}"
            );
            assert_eq!(result.points(), [warm.from(), warm.to()], "{label}");
            assert_eq!(
                result.path_type(),
                1,
                "same-polygon source replaces old type {label}"
            );
        } else {
            assert!(result.smooth_stop().is_some(), "{label}");
        }
        if result.smooth_points().len() == 1 && result.smooth_status() == Some(SUCCESS) {
            assert_eq!(
                result.smooth_stop(),
                Some(SourceSmoothStop::NoSteerTarget),
                "{label}"
            );
            assert_eq!(
                result.path_type(),
                8,
                "single-point success becomes NOPATH {label}"
            );
            assert_eq!(result.after().active_len(), 0, "{label}");
            early_stop += 1;
        }
        if result.smooth_points().len() == 74 && result.smooth_status() == Some(FAILURE) {
            assert_eq!(result.path_type(), 8, "raw failure precedes SHORT {label}");
            assert_eq!(result.after().active_len(), 0, "{label}");
            failed_limit += 1;
        }
        optimized += usize::from(
            config.optimize_path && result.points().len() < result.smooth_points().len(),
        );
        preserved_incomplete +=
            usize::from(config.previous_path_type & 4 != 0 && result.path_type() == 4);
        tails += usize::from(result.after().storage().len() > 74);
        phases.insert(row.get("phase").to_owned());
        types.insert(result.path_type());
        let retry = mesh
            .loaded_source_warm_point_path(&warm, config, SourcePointPathBudget::default(), until())
            .unwrap();
        assert_eq!(retry, result, "deterministic repeated point path {label}");
        assert_eq!(warm, prior, "retry mutated selected input {label}");
        let terminal = result.after().clone();
        assert_eq!(
            result.into_after(),
            terminal,
            "owned terminal proposal {label}"
        );
        accepted += 1;
    }
    assert!(accepted >= 8, "only {accepted} admitted source rows");
    assert!(optimized > 0, "optimization never changed a loaded path");
    assert!(
        preserved_incomplete > 0,
        "prior INCOMPLETE was not exercised"
    );
    assert!(
        failed_limit > 0,
        "raw failure at 74 did not exercise NOPATH-before-SHORT"
    );
    assert!(early_stop > 0, "no loaded one-point NoSteerTarget");
    assert!(
        tails > 0,
        "no inactive searchable tail survived terminal handling"
    );
    for phase in ["same", "cut", "replacement"] {
        assert!(phases.contains(phase), "missing selected phase {phase}");
    }
    for path_type in [1, 4, 8, 17] {
        assert!(
            types.contains(&path_type),
            "missing terminal type {path_type}"
        );
    }
}

#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
struct RawTerminal {
    points: [[f32; 3]; 74],
    actual_end: [f32; 3],
    point_count: u32,
    path_type: u32,
    clear_cache: u32,
}
unsafe extern "C" {
    fn benilla_source_point_path(
        yzx: *const f32,
        count: u32,
        limit: u32,
        status: u32,
        path_type: u32,
        start: *const f32,
        end: *const f32,
        optimize: u32,
        force: u32,
        result: *mut RawTerminal,
    ) -> u32;
}

// Raw numeric controls exercise no mesh or opaque cache constructors. They are not a
// producer for the composed API; expectations come from actual original BuildPointPath.
fn raw_terminal(row: &OracleRow<'_>) -> (u32, RawTerminal) {
    let points: Vec<_> = row
        .points("raw_points")
        .into_iter()
        .map(|p| detour(p).unwrap())
        .collect();
    let mut result = RawTerminal {
        points: [[1234.; 3]; 74],
        actual_end: [1234.; 3],
        point_count: 999,
        path_type: 999,
        clear_cache: 999,
    };
    // SAFETY: fixed C-layout output, bounded source fixture points and no retained pointers.
    let status = unsafe {
        benilla_source_point_path(
            points.as_ptr().cast(),
            points.len() as u32,
            row.count("limit") as u32,
            row.hex("raw_status"),
            row.count("previous_type") as u32,
            row.point("from").map(f32::from_bits).as_ptr(),
            row.point("to").map(f32::from_bits).as_ptr(),
            u32::from(row.flag("optimize")),
            u32::from(row.flag("force")),
            &mut result,
        )
    };
    (status, result)
}

#[test]
fn original_postprocess_controls_match_mutated_predecessor_thresholds_failure_and_force_ordering() {
    let mut labels = BTreeSet::new();
    let mut statuses = BTreeSet::new();
    let mut zero_rejections = 0;
    let mut short = 0;
    let mut failure = 0;
    let mut force = 0;
    for row in raw_cases() {
        let label = row.get("label");
        assert!(
            labels.insert(label.to_owned()),
            "duplicate source control {label}"
        );
        assert!(
            !row.flag("normalize") || row.flag("ignore"),
            "unadmitted normalization {label}"
        );
        let (status, result) = raw_terminal(&row);
        if row.flag("optimize") && original_zero(&row.points("raw_points")) {
            assert_eq!(
                status, 2,
                "original zero needs omitted retained point-vector state: {label}"
            );
            assert_eq!(
                result.point_count, 999,
                "rejected numeric output was published {label}"
            );
            zero_rejections += 1;
            continue;
        }
        assert_eq!(status, 1, "{label}");
        let actual: Vec<_> = result.points[..result.point_count as usize]
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect();
        assert_eq!(
            actual,
            row.points("points"),
            "original final point bits {label}"
        );
        assert_eq!(
            result.actual_end.map(f32::to_bits),
            row.point("actual_end"),
            "{label}"
        );
        assert_eq!(result.path_type, row.count("path_type") as u32, "{label}");
        assert_eq!(
            row.refs("before_storage"),
            row.refs("after_storage"),
            "source terminal must retain inactive slots {label}"
        );
        assert_eq!(
            if result.clear_cache == 1 {
                0
            } else {
                row.count("before_active")
            },
            row.count("after_active"),
            "source terminal active length {label}"
        );
        short += usize::from(result.path_type == 32);
        failure += usize::from(result.path_type == 8);
        force += usize::from(result.path_type == 17);
        statuses.insert(row.hex("raw_status"));
    }
    assert!(labels.len() >= 12, "insufficient original numeric controls");
    assert!(
        zero_rejections > 0,
        "no original-zero retained-state exclusion"
    );
    assert!(
        short > 0,
        "source success at optimized point limit must be SHORT"
    );
    assert!(failure > 0, "source failure ordering untested");
    assert!(
        force >= 2,
        "both force-destination outcomes need observations"
    );
    assert!(statuses.contains(&SUCCESS) && statuses.contains(&FAILURE));
}

#[test]
fn point_path_budgets_and_expiry_never_publish_or_poison_retry() {
    let mut tested = 0;
    for row in cases() {
        if row.config().optimize_path && original_zero(&row.points("smooth_points")) {
            continue;
        }
        let mut mesh = oracle_mesh(row.get("geometry"));
        let warm = selected(&mut mesh, &row);
        let prior = warm.try_copy().unwrap();
        let baseline = mesh
            .loaded_source_warm_point_path(
                &warm,
                row.config(),
                SourcePointPathBudget::default(),
                until(),
            )
            .unwrap();
        let smooth_work = baseline.diagnostics().smoothing.primitive_calls;
        let post_work = baseline.diagnostics().postprocess_primitives;
        assert_eq!(
            mesh.loaded_source_warm_point_path(
                &warm,
                row.config(),
                SourcePointPathBudget::default(),
                Instant::now()
            )
            .unwrap_err(),
            SourcePointPathError::Boundary(Error::Deadline)
        );
        for cap in 0..smooth_work {
            let error = mesh
                .loaded_source_warm_point_path(
                    &warm,
                    row.config(),
                    SourcePointPathBudget {
                        smoothing_primitives: cap,
                        postprocess_primitives: post_work,
                    },
                    until(),
                )
                .unwrap_err();
            let SourcePointPathError::Smoothing(error) = error else {
                panic!("{} cap {cap}: {error:?}", row.get("label"));
            };
            assert_eq!(error.error, Error::Budget);
            assert_eq!(error.diagnostics.primitive_calls, cap);
            assert_eq!(warm, prior, "cancelled smoothing changed selection");
        }
        for cap in [0, post_work - 1] {
            assert_eq!(
                mesh.loaded_source_warm_point_path(
                    &warm,
                    row.config(),
                    SourcePointPathBudget {
                        smoothing_primitives: smooth_work,
                        postprocess_primitives: cap
                    },
                    until()
                )
                .unwrap_err(),
                SourcePointPathError::Boundary(Error::Budget)
            );
            assert_eq!(warm, prior, "cancelled postprocessing changed selection");
        }
        for budget in [
            SourcePointPathBudget {
                smoothing_primitives: SOURCE_SMOOTHING_MAX_PRIMITIVES + 1,
                postprocess_primitives: post_work,
            },
            SourcePointPathBudget {
                smoothing_primitives: smooth_work,
                postprocess_primitives: SOURCE_POINT_PATH_MAX_POSTPROCESS_PRIMITIVES + 1,
            },
        ] {
            assert_eq!(
                mesh.loaded_source_warm_point_path(&warm, row.config(), budget, until())
                    .unwrap_err(),
                SourcePointPathError::Boundary(Error::Budget)
            );
        }
        let retry = mesh
            .loaded_source_warm_point_path(
                &warm,
                row.config(),
                SourcePointPathBudget {
                    smoothing_primitives: smooth_work,
                    postprocess_primitives: post_work,
                },
                until(),
            )
            .unwrap();
        assert_eq!(
            retry,
            baseline,
            "exact work cap and cancellation retry {}",
            row.get("label")
        );
        assert_eq!(warm, prior);
        tested += 1;
    }
    assert!(tested >= 8);
}

#[test]
fn point_path_requires_same_loaded_instance_and_explicit_inactive_normalization() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 0.);
    let to = point(-400., -200., 0.);
    let cache = mesh
        .loaded_source_corridor(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap()
        .warm_cache_seed()
        .unwrap();
    let warm = mesh
        .loaded_source_warm_cache(&cache, from, to, SourceWarmCacheBudget::default(), until())
        .unwrap();
    let prior = warm.try_copy().unwrap();
    let config = SourcePointPathConfig {
        optimize_path: true,
        normalize_z: false,
        ignore_normalization: false,
        force_destination: false,
        previous_path_type: 0,
    };
    let baseline = mesh
        .loaded_source_warm_point_path(&warm, config, SourcePointPathBudget::default(), until())
        .unwrap();
    for (normalize_z, ignore_normalization) in [(false, false), (false, true), (true, true)] {
        let config = SourcePointPathConfig {
            normalize_z,
            ignore_normalization,
            ..config
        };
        let result = mesh
            .loaded_source_warm_point_path(&warm, config, SourcePointPathBudget::default(), until())
            .unwrap();
        assert_eq!(result.points(), baseline.points());
        assert_eq!(result.actual_end(), baseline.actual_end());
        assert_eq!(result.after(), baseline.after());
        assert_eq!(result.config(), config);
    }
    assert_eq!(
        mesh.loaded_source_warm_point_path(
            &warm,
            SourcePointPathConfig {
                normalize_z: true,
                ignore_normalization: false,
                ..config
            },
            SourcePointPathBudget::default(),
            until()
        )
        .unwrap_err(),
        SourcePointPathError::ActiveNormalizationUnsupported
    );
    for previous_path_type in [0x40, 0x8000_0000, u32::MAX] {
        assert_eq!(
            mesh.loaded_source_warm_point_path(
                &warm,
                SourcePointPathConfig {
                    previous_path_type,
                    ..config
                },
                SourcePointPathBudget::default(),
                until()
            )
            .unwrap_err(),
            SourcePointPathError::InvalidPreviousPathType
        );
    }
    let mut identical = existing_mesh();
    let mut missing = Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: LEFT,
        }],
    )
    .unwrap();
    let mut changed = oracle_mesh("ground");
    for other in [&mut identical, &mut missing, &mut changed] {
        assert_eq!(
            other
                .loaded_source_warm_point_path(
                    &warm,
                    config,
                    SourcePointPathBudget::default(),
                    until()
                )
                .unwrap_err(),
            SourcePointPathError::Boundary(Error::Identity)
        );
    }
    assert_eq!(warm, prior);
    assert_eq!(
        mesh.loaded_source_warm_point_path(
            &warm,
            config,
            SourcePointPathBudget::default(),
            until()
        )
        .unwrap(),
        baseline
    );
    let legacy = mesh.route(from, to, until()).unwrap();
    assert!(!legacy.polygons.is_empty());
    drop(mesh);
    for _ in 0..4 {
        let mut reloaded = existing_mesh();
        assert_eq!(
            reloaded
                .loaded_source_warm_point_path(
                    &warm,
                    config,
                    SourcePointPathBudget::default(),
                    until()
                )
                .unwrap_err(),
            SourcePointPathError::Boundary(Error::Identity)
        );
    }
}

#[test]
fn original_numeric_controls_pin_strict_cut_threshold_and_mutated_zero_predecessor() {
    let rows = raw_cases();
    let row = |label: &str| {
        rows.iter()
            .find(|row| row.get("label") == label)
            .unwrap_or_else(|| panic!("missing original control {label}"))
    };
    let below = row("line_fault_below");
    let equal = row("line_fault_equal");
    let above = row("line_fault_above");
    // The three inputs straddle an exactly representable 0.5 line determinant.
    assert_eq!(
        below.points("raw_points")[1][1] + 1,
        equal.points("raw_points")[1][1]
    );
    assert_eq!(
        equal.points("raw_points")[1][1] + 1,
        above.points("raw_points")[1][1]
    );
    assert_eq!(below.points("points").len(), 2);
    assert_eq!(equal.points("points").len(), 3);
    assert_eq!(above.points("points").len(), 3);
    let six = row("skip_limit_six");
    let raw = six.points("raw_points");
    assert_eq!(six.points("points"), [raw[0], raw[7], raw[14], raw[16]]);
    let mutated = row("mutated_previous_not_original_collinearity");
    let raw = mutated.points("raw_points");
    // Removing index one changes index two's predecessor to XYZ zero. A conventional
    // original-neighbor or last-kept-neighbor simplifier would remove this middle point.
    assert_eq!(mutated.points("points"), [raw[0], raw[2], raw[4]]);
    let optimized_limit = row("raw_optimize_before_limit");
    let optimized_failure = row("raw_failure_after_optimize");
    assert_eq!(optimized_limit.points("raw_points").len(), 74);
    assert!(optimized_limit.points("points").len() < 74);
    assert_eq!(optimized_limit.count("path_type"), 1);
    assert_eq!(optimized_limit.hex("raw_status"), SUCCESS);
    assert_eq!(
        optimized_failure.points("raw_points"),
        optimized_limit.points("raw_points")
    );
    assert_eq!(optimized_failure.hex("raw_status"), FAILURE);
    assert_eq!(optimized_failure.count("path_type"), 8);
    assert_eq!(optimized_failure.count("after_active"), 0);
    let fresh = row("zero_input_fresh_default");
    let retained = row("zero_input_prior_survives");
    assert_eq!(fresh.points("raw_points"), retained.points("raw_points"));
    assert_ne!(fresh.points("points"), retained.points("points"));
    assert_eq!(fresh.points("points")[1], [0; 3]);
    assert_eq!(
        retained.points("points")[1],
        retained.points("prior_points")[1]
    );
    assert_eq!(row("force_ratio_below").count("after_active"), 1);
    assert_eq!(row("force_ratio_equal").count("after_active"), 0);
    assert_eq!(row("force_ratio_above").count("after_active"), 0);
    for axis in ["radius", "height"] {
        assert_eq!(row(&format!("range_{axis}_below")).count("path_type"), 1);
        assert_eq!(row(&format!("range_{axis}_equal")).count("path_type"), 17);
        assert_eq!(row(&format!("range_{axis}_above")).count("path_type"), 17);
    }
}

#[test]
fn actual_warm_selected_reference_is_used_even_when_fresh_nearest_is_different() {
    let rows = cases();
    let row = rows
        .iter()
        .find(|row| row.get("label") == "warm_active_before_nearer_floor")
        .expect("missing source warm-selection discriminator");
    let mut mesh = oracle_mesh(row.get("geometry"));
    let warm = selected(&mut mesh, row);
    assert_eq!(warm.start().origin, SourceWarmLookupOrigin::Active);
    let nearest = mesh.nearest(warm.from(), 5., until()).unwrap();
    assert_ne!(
        warm.start().polygon,
        nearest.polygon,
        "fixture must discriminate selected warm refs from fresh lookup"
    );
    let result = mesh
        .loaded_source_warm_point_path(
            &warm,
            row.config(),
            SourcePointPathBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(result.warm().start().polygon, warm.start().polygon);
    assert_eq!(result.smooth_points(), row.points("smooth_points"));
    assert_eq!(result.points(), row.points("points"));
    assert_eq!(result.after().storage(), row.refs("after_storage"));
}

#[test]
fn loaded_original_zero_optimization_rejects_without_rejecting_raw_conversion_or_same_polygon() {
    let rows = cases();
    let row = rows
        .iter()
        .find(|row| row.get("label") == "original_zero_input_raw_observation")
        .expect("missing loaded original-zero observation");
    let mut mesh = oracle_mesh(row.get("geometry"));
    let warm = selected(&mut mesh, row);
    let prior = warm.try_copy().unwrap();
    assert!(original_zero(&row.points("smooth_points")));
    for _ in 0..2 {
        assert_eq!(
            mesh.loaded_source_warm_point_path(
                &warm,
                row.config(),
                SourcePointPathBudget::default(),
                until()
            )
            .unwrap_err(),
            SourcePointPathError::OriginalZeroPointOptimizationUnsupported
        );
        assert_eq!(warm, prior);
    }
    let raw = mesh
        .loaded_source_warm_point_path(
            &warm,
            SourcePointPathConfig {
                optimize_path: false,
                ..row.config()
            },
            SourcePointPathBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(raw.points(), row.points("smooth_points"));
    assert_eq!(raw.smooth_points(), row.points("smooth_points"));
    assert_eq!(raw.path_type(), 1);
    assert_eq!(warm, prior);
    let same = rows
        .iter()
        .find(|row| row.get("label") == "same_polygon_source_order")
        .expect("missing same-polygon source ordering");
    let warm = selected(&mut mesh, same);
    assert_eq!(warm.phase(), SourceWarmPolyPhase::SamePolygon);
    let result = mesh
        .loaded_source_warm_point_path(
            &warm,
            same.config(),
            SourcePointPathBudget {
                smoothing_primitives: 0,
                postprocess_primitives: 1,
            },
            until(),
        )
        .unwrap();
    assert_eq!(result.points(), [warm.from(), warm.to()]);
    assert_eq!(result.path_type(), 1);
    assert_eq!(result.smooth_status(), None);
    assert_eq!(result.smooth_stop(), None);
    assert_eq!(
        result.diagnostics().smoothing,
        SourceSmoothingDiagnostics::default()
    );
}

#[test]
fn terminal_warm_branch_preserves_separate_fresh_corridor_raw_smoothing_api() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 0.);
    let to = point(-400., -200., 0.);
    let corridor = mesh
        .loaded_source_corridor(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap();
    let cache = corridor.warm_cache_seed().unwrap();
    let baseline = corridor.smooth().unwrap();
    let legacy = mesh.route(from, to, until()).unwrap();
    let warm = mesh
        .loaded_source_warm_cache(&cache, from, to, SourceWarmCacheBudget::default(), until())
        .unwrap();
    let config = SourcePointPathConfig {
        optimize_path: true,
        normalize_z: false,
        ignore_normalization: false,
        force_destination: false,
        previous_path_type: 0,
    };
    let terminal = mesh
        .loaded_source_warm_point_path(&warm, config, SourcePointPathBudget::default(), until())
        .unwrap();
    assert_eq!(terminal.smooth_points(), baseline.points);
    assert!(terminal.points().len() < baseline.points.len());
    assert_eq!(terminal.after().storage(), cache.storage());
    let repeated = mesh
        .loaded_source_corridor(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap()
        .smooth()
        .unwrap();
    assert_eq!(repeated, baseline);
    assert_eq!(mesh.route(from, to, until()).unwrap(), legacy);
}
