// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded warm-cache dependency tests against original PathFinder/Detour.
//! Authored cache storage is test-only; no owner initial-cache or movement admission.
use super::*;
use std::{collections::BTreeSet, time::Duration};

const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const LEFT: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
const RIGHT: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
const DIRECT_PARAMS: &[u8] =
    include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture");
const GROUND: SourceOwnerFilter = SourceOwnerFilter {
    include_flags: 1,
    exclude_flags: 0,
};

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
    let tiles = match geometry {
        "existing" => return existing_mesh(),
        "long600" => vec![TileInput {
            grid: [32, 32],
            bytes: include_bytes!("../tools/fixtures/direct-corridor/long600.test-fixture"),
        }],
        "long1100" => vec![TileInput {
            grid: [32, 32],
            bytes: include_bytes!("../tools/fixtures/direct-corridor/long1100.test-fixture"),
        }],
        "grid" => vec![
            TileInput {
                grid: [32, 32],
                bytes: include_bytes!("../tools/fixtures/direct-corridor/grid00.test-fixture"),
            },
            TileInput {
                grid: [32, 31],
                bytes: include_bytes!("../tools/fixtures/direct-corridor/grid10.test-fixture"),
            },
            TileInput {
                grid: [31, 32],
                bytes: include_bytes!("../tools/fixtures/direct-corridor/grid01.test-fixture"),
            },
            TileInput {
                grid: [31, 31],
                bytes: include_bytes!("../tools/fixtures/direct-corridor/grid11.test-fixture"),
            },
        ],
        "precision" => vec![TileInput {
            grid: [31, 31],
            bytes: include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
        }],
        other => panic!("unknown authored warm-cache geometry {other}"),
    };
    Mesh::load(DIRECT_PARAMS, &tiles).unwrap()
}

fn seed(mesh: &mut Mesh, from: [u32; 3], to: [u32; 3]) -> WarmCache {
    mesh.loaded_source_corridor(
        from,
        to,
        GROUND,
        74,
        SourceSmoothingBudget::default(),
        until(),
    )
    .unwrap()
    .warm_cache_seed()
    .unwrap()
}

fn snapshot(cache: &WarmCache) -> (Vec<u64>, usize, usize, SourceOwnerFilter) {
    (
        cache.storage().to_vec(),
        cache.active_len(),
        cache.point_limit(),
        cache.filter(),
    )
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
            .unwrap_or_else(|| panic!("missing oracle field {name}"))]
    }
    fn hex(&self, name: &str) -> u32 {
        u32::from_str_radix(self.get(name), 16).unwrap()
    }
    fn count(&self, name: &str) -> usize {
        self.get(name).parse().unwrap()
    }
    fn point(&self, name: &str) -> [u32; 3] {
        self.get(name)
            .split(',')
            .map(|v| u32::from_str_radix(v, 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }
    fn refs(&self, name: &str) -> Vec<u64> {
        match self.get(name) {
            "-" => Vec::new(),
            refs => refs.split(',').map(|v| v.parse().unwrap()).collect(),
        }
    }
}

fn oracle_rows(fixture: &'static str) -> Vec<OracleRow<'static>> {
    let headings: Vec<_> = fixture
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .expect("missing original warm-cache oracle schema");
    fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            assert_eq!(
                values.len(),
                headings.len(),
                "warm-cache oracle row: {line}"
            );
            OracleRow {
                headings: headings.clone(),
                values,
            }
        })
        .collect()
}

#[test]
fn warm_seed_retains_original_corridor_before_smoothing_and_fixed_storage() {
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
    let original = corridor.polygons().to_vec();
    let cache = corridor.warm_cache_seed().unwrap();
    assert_eq!(cache.point_limit(), 74);
    assert_eq!(cache.active_len(), original.len());
    assert_eq!(cache.storage().len(), 74);
    assert_eq!(&cache.storage()[..cache.active_len()], original);
    assert!(cache.storage()[cache.active_len()..]
        .iter()
        .all(|reference| *reference == 0));
    assert_eq!(cache.filter(), GROUND);
    let before = snapshot(&cache);
    let smooth = corridor.smooth().unwrap();
    assert_eq!(smooth.polygons, original);
    assert_ne!(
        smooth.final_polygons, original,
        "fixture did not consume its corridor"
    );
    assert_eq!(snapshot(&cache), before);
    let result = mesh
        .loaded_source_warm_cache(&cache, from, to, SourceWarmCacheBudget::default(), until())
        .unwrap();
    assert_eq!(result.phase(), SourceWarmPolyPhase::Cut);
    assert_eq!(
        &result.after().storage()[..result.after().active_len()],
        original
    );
    assert_eq!(snapshot(&cache), before);
    for limit in [2, 73] {
        let corridor = mesh
            .loaded_source_corridor(
                from,
                to,
                GROUND,
                limit,
                SourceSmoothingBudget::default(),
                until(),
            )
            .unwrap();
        assert_eq!(corridor.warm_cache_seed().unwrap_err(), Error::Unsupported);
    }
}

#[test]
fn identical_bytes_different_mesh_missing_tiles_and_changed_geometry_reject_dependency() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 0.);
    let to = point(-400., 100., 2.);
    let cache = seed(&mut mesh, from, to);
    let before = snapshot(&cache);
    let mut identical = existing_mesh();
    let mut missing = Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: LEFT,
        }],
    )
    .unwrap();
    let mut changed = oracle_mesh("long600");
    for other in [&mut identical, &mut missing, &mut changed] {
        assert_eq!(
            other
                .loaded_source_warm_cache(
                    &cache,
                    from,
                    to,
                    SourceWarmCacheBudget::default(),
                    until()
                )
                .unwrap_err(),
            SourceWarmCacheError::Boundary(Error::Identity)
        );
    }
    assert!(
        WarmCache::authored_fixture(&missing, cache.storage(), cache.active_len()).is_err(),
        "unloaded cached reference admitted"
    );
    assert_eq!(snapshot(&cache), before);
    let result = mesh
        .loaded_source_warm_cache(&cache, from, to, SourceWarmCacheBudget::default(), until())
        .unwrap();
    assert_eq!(result.phase(), SourceWarmPolyPhase::Cut);
    assert_eq!(snapshot(&cache), before);
    drop(mesh);
    for _ in 0..4 {
        let mut reloaded = existing_mesh();
        assert_eq!(
            reloaded
                .loaded_source_warm_cache(
                    &cache,
                    from,
                    to,
                    SourceWarmCacheBudget::default(),
                    until()
                )
                .unwrap_err(),
            SourceWarmCacheError::Boundary(Error::Identity)
        );
    }
}

#[test]
fn cold_distance_nonfinite_input_and_expiry_reject_without_mutation_or_query_poisoning() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 0.);
    let to = point(-400., -100., 0.);
    let cache = seed(&mut mesh, from, to);
    let before = snapshot(&cache);
    let legacy = mesh.route(from, to, until()).unwrap();
    let cold = WarmCache::authored_fixture(&mesh, &[0; 74], 0).unwrap();
    assert_eq!(
        mesh.loaded_source_warm_cache(&cold, from, to, SourceWarmCacheBudget::default(), until())
            .unwrap_err(),
        SourceWarmCacheError::ColdDistanceUndefined
    );
    assert_eq!(snapshot(&cold), (vec![0; 74], 0, 74, GROUND));
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 100_001.] {
        for (a, b) in [(point(bad, -400., 0.), to), (from, point(-400., bad, 0.))] {
            assert_eq!(
                mesh.loaded_source_warm_cache(
                    &cache,
                    a,
                    b,
                    SourceWarmCacheBudget::default(),
                    until()
                )
                .unwrap_err(),
                SourceWarmCacheError::Boundary(Error::Format)
            );
            assert_eq!(snapshot(&cache), before);
            assert_eq!(mesh.route(from, to, until()).unwrap(), legacy);
        }
    }
    assert_eq!(
        mesh.loaded_source_warm_cache(
            &cache,
            from,
            to,
            SourceWarmCacheBudget::default(),
            Instant::now()
        )
        .unwrap_err(),
        SourceWarmCacheError::Boundary(Error::Deadline)
    );
    assert_eq!(
        mesh.loaded_source_warm_cache(
            &cache,
            from,
            to,
            SourceWarmCacheBudget {
                lookup_primitives: 0,
                corridor_expansions: 0
            },
            until()
        )
        .unwrap_err(),
        SourceWarmCacheError::Boundary(Error::Budget)
    );
    assert_eq!(snapshot(&cache), before);
    assert_eq!(mesh.route(from, to, until()).unwrap(), legacy);
    let retry = mesh
        .loaded_source_warm_cache(&cache, from, to, SourceWarmCacheBudget::default(), until())
        .unwrap();
    assert_eq!(retry.phase(), SourceWarmPolyPhase::Cut);
    assert_eq!(snapshot(&cache), before);
}

fn origin(value: &str) -> SourceWarmLookupOrigin {
    match value {
        "active" => SourceWarmLookupOrigin::Active,
        "tail" => SourceWarmLookupOrigin::Tail,
        "nearest5" => SourceWarmLookupOrigin::Nearest5,
        "nearest10" => SourceWarmLookupOrigin::Nearest10,
        other => panic!("unknown source lookup origin {other}"),
    }
}

fn lookup(row: &OracleRow<'_>, prefix: &str) -> SourceWarmLookup {
    SourceWarmLookup {
        polygon: row.get(&format!("{prefix}_ref")).parse().unwrap(),
        distance_bits: row.hex(&format!("{prefix}_distance")),
        origin: origin(row.get(&format!("{prefix}_origin"))),
        appended: match row.get(&format!("{prefix}_appended")) {
            "0" => false,
            "1" => true,
            other => panic!("invalid source append flag {other}"),
        },
    }
}

fn cases() -> Vec<OracleRow<'static>> {
    oracle_rows(include_str!(
        "../tools/fixtures/warm-cache/cases.test-fixture"
    ))
}

#[test]
fn original_loaded_warm_cases_match_selected_refs_distance_bits_and_entire_storage() {
    let mut labels = BTreeSet::new();
    let mut origins = BTreeSet::new();
    let mut phases = BTreeSet::new();
    let mut statuses = BTreeSet::new();
    let mut accepted = 0;
    let mut rejected = 0;
    let mut tail_preserved = 0;
    let mut start_append_end_tail = 0;
    for row in cases() {
        let label = row.get("label");
        assert!(
            labels.insert(label.to_owned()),
            "duplicate source case {label}"
        );
        if row.count("limit") != 74 {
            continue;
        }
        let mut mesh = oracle_mesh(row.get("geometry"));
        let before = WarmCache::authored_fixture(
            &mesh,
            &row.refs("before_storage"),
            row.count("before_active"),
        )
        .unwrap();
        let prior = snapshot(&before);
        let result = mesh.loaded_source_warm_cache(
            &before,
            row.point("from"),
            row.point("to"),
            SourceWarmCacheBudget::default(),
            until(),
        );
        assert_eq!(
            snapshot(&before),
            prior,
            "source proposal changed input: {label}"
        );
        let expected_error = if before.active_len() == 0 && before.storage().len() <= 74 {
            Some(SourceWarmCacheError::ColdDistanceUndefined)
        } else if row.get("start_ref") == "0" || row.get("end_ref") == "0" {
            Some(SourceWarmCacheError::MissingPolygon)
        } else if f32::from_bits(row.hex("start_distance")) > 7.
            || f32::from_bits(row.hex("end_distance")) > 7.
        {
            Some(SourceWarmCacheError::FarFromPolygonUnsupported)
        } else if row.get("direct_status") != "-" && row.hex("direct_status") != SUCCESS {
            statuses.insert(row.hex("direct_status"));
            Some(SourceWarmCacheError::UnsupportedDirectStatus(
                row.hex("direct_status"),
            ))
        } else {
            None
        };
        if let Some(error) = expected_error {
            assert_eq!(result.unwrap_err(), error, "{label}");
            assert_eq!(
                mesh.loaded_source_warm_cache(
                    &before,
                    row.point("from"),
                    row.point("to"),
                    SourceWarmCacheBudget::default(),
                    until()
                )
                .unwrap_err(),
                error,
                "repeat rejection {label}"
            );
            assert_eq!(snapshot(&before), prior, "repeat rejection mutated {label}");
            rejected += 1;
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        assert_eq!(result.from(), row.point("from"), "{label}");
        assert_eq!(result.to(), row.point("to"), "{label}");
        assert_eq!(result.start(), lookup(&row, "start"), "{label}");
        assert_eq!(result.end(), lookup(&row, "end"), "{label}");
        let expected_phase = match row.get("phase") {
            "same" => SourceWarmPolyPhase::SamePolygon,
            "cut" => SourceWarmPolyPhase::Cut,
            "replacement" => SourceWarmPolyPhase::Replacement,
            other => panic!("unexpected admitted phase {other}: {label}"),
        };
        assert_eq!(result.phase(), expected_phase, "{label}");
        assert_eq!(
            result.direct_status(),
            (row.get("direct_status") != "-").then(|| row.hex("direct_status")),
            "{label}"
        );
        assert_eq!(
            result.after().storage(),
            row.refs("after_storage"),
            "full retained storage {label}"
        );
        assert_eq!(
            result.after().active_len(),
            row.count("after_active"),
            "{label}"
        );
        assert_eq!(result.after().point_limit(), 74, "{label}");
        assert_eq!(result.after().filter(), GROUND, "{label}");
        assert_eq!(snapshot(&before), prior, "{label}");
        let retry = mesh
            .loaded_source_warm_cache(
                &before,
                row.point("from"),
                row.point("to"),
                SourceWarmCacheBudget::default(),
                until(),
            )
            .unwrap();
        assert_eq!(retry, result, "deterministic repeat {label}");
        let after = snapshot(result.after());
        assert_eq!(
            snapshot(&result.into_after()),
            after,
            "owned result {label}"
        );
        origins.extend([
            row.get("start_origin").to_owned(),
            row.get("end_origin").to_owned(),
        ]);
        phases.insert(row.get("phase").to_owned());
        tail_preserved += usize::from(row.refs("after_storage").len() > 74);
        start_append_end_tail +=
            usize::from(row.get("start_appended") == "1" && row.get("end_origin") == "tail");
        accepted += 1;
    }
    assert!(accepted >= 12, "only {accepted} admitted loaded cases");
    assert!(rejected >= 4, "only {rejected} rejected original cases");
    for expected in ["active", "tail", "nearest5", "nearest10"] {
        assert!(
            origins.contains(expected),
            "missing source lookup origin {expected}"
        );
    }
    for expected in ["same", "cut", "replacement"] {
        assert!(
            phases.contains(expected),
            "missing source polygon phase {expected}"
        );
    }
    assert!(tail_preserved >= 3, "retained tail not exercised");
    assert!(
        start_append_end_tail >= 1,
        "sequential append was not visible to end lookup"
    );
    assert!(
        statuses.iter().any(|status| status & (1 << 4) != 0),
        "no raw truncation rejection"
    );
    assert!(
        statuses.iter().any(|status| status & (1 << 6) != 0),
        "no raw partial rejection"
    );
}

#[test]
fn exact_lookup_and_direct_expansion_caps_cancel_atomically_and_leave_legacy_query_reusable() {
    let mut phases = BTreeSet::new();
    for row in cases() {
        if row.count("limit") != 74
            || row.get("phase") == "excluded"
            || (row.get("direct_status") != "-" && row.hex("direct_status") != SUCCESS)
        {
            continue;
        }
        let mut mesh = oracle_mesh(row.get("geometry"));
        let before = WarmCache::authored_fixture(
            &mesh,
            &row.refs("before_storage"),
            row.count("before_active"),
        )
        .unwrap();
        let prior = snapshot(&before);
        let from = row.point("from");
        let to = row.point("to");
        let baseline = mesh
            .loaded_source_warm_cache(&before, from, to, SourceWarmCacheBudget::default(), until())
            .unwrap();
        let legacy = mesh.route(from, to, until());
        let expansions = baseline
            .trace()
            .iter()
            .filter(|call| call.primitive == SourceWarmPrimitive::DirectExpansion)
            .count();
        let lookups = baseline.trace().len() - expansions;
        let exact = SourceWarmCacheBudget {
            lookup_primitives: lookups,
            corridor_expansions: expansions,
        };
        assert_eq!(
            mesh.loaded_source_warm_cache(&before, from, to, exact, until())
                .unwrap(),
            baseline,
            "exact budget {}",
            row.get("label")
        );
        let mut insufficient = vec![SourceWarmCacheBudget {
            lookup_primitives: lookups - 1,
            ..exact
        }];
        if expansions > 0 {
            insufficient.push(SourceWarmCacheBudget {
                corridor_expansions: expansions - 1,
                ..exact
            });
        }
        for budget in insufficient {
            assert_eq!(
                mesh.loaded_source_warm_cache(&before, from, to, budget, until())
                    .unwrap_err(),
                SourceWarmCacheError::Boundary(Error::Budget),
                "short budget {}",
                row.get("label")
            );
            assert_eq!(snapshot(&before), prior);
            assert_eq!(
                mesh.route(from, to, until()),
                legacy,
                "legacy query after cancellation {}",
                row.get("label")
            );
            assert_eq!(
                mesh.loaded_source_warm_cache(&before, from, to, exact, until())
                    .unwrap(),
                baseline,
                "retry after cancellation {}",
                row.get("label")
            );
        }
        phases.insert(row.get("phase").to_owned());
    }
    assert_eq!(phases.len(), 3);
}

#[repr(C)]
#[derive(Default)]
struct RawScan {
    reference: u64,
    minimum_squared: f32,
    distance: f32,
}
unsafe extern "C" {
    fn benilla_nav_warm_scan_reset(scan: *mut RawScan);
    fn benilla_nav_warm_scan(
        mesh: *mut std::ffi::c_void,
        reference: u64,
        point: *const f32,
        scan: *mut RawScan,
    ) -> u32;
}

#[test]
fn loaded_source_scan_preserves_order_zero_ties_and_neighboring_squared_thresholds() {
    let mut labels = BTreeSet::new();
    let mut thresholds = BTreeSet::new();
    let mut matches = 0;
    for row in oracle_rows(include_str!(
        "../tools/fixtures/warm-cache/scans.test-fixture"
    )) {
        let label = row.get("label");
        assert!(
            labels.insert(label.to_owned()),
            "duplicate raw scan case {label}"
        );
        let mesh = oracle_mesh(row.get("geometry"));
        let refs = row.refs("refs");
        assert!(!refs.is_empty());
        let mut storage = refs.clone();
        storage.resize(storage.len().max(74), 0);
        // Test-only authored refs must still belong to this loaded native mesh.
        WarmCache::authored_fixture(&mesh, &storage, refs.len()).unwrap();
        let input = detour(row.point("point")).unwrap();
        let mut scan = RawScan::default();
        // SAFETY: initialized C-layout output and no retained native pointers.
        unsafe { benilla_nav_warm_scan_reset(&mut scan) };
        let squared: Vec<_> = row
            .get("squared_distances")
            .split(',')
            .map(|value| {
                if value == "-" {
                    None
                } else {
                    Some(u32::from_str_radix(value, 16).unwrap())
                }
            })
            .collect();
        assert_eq!(squared.len(), refs.len(), "{label}");
        for nominal in [1f32, 3., 7.] {
            for (suffix, offset) in [("below", -1i64), ("at", 0), ("above", 1)] {
                if label == format!("squared{}_{}", nominal as u32, suffix) {
                    assert_eq!(
                        squared[0],
                        Some((i64::from(nominal.to_bits()) + offset) as u32),
                        "exact adjacent squared witness {label}"
                    );
                }
            }
        }
        let mut minimum = f32::MAX;
        for (reference, expected_squared) in refs.into_iter().zip(squared) {
            // SAFETY: fixture validation checked refs on this live mesh, zero has defined
            // native failure, and the finite input plus scan have their exact C layout.
            let status = unsafe {
                benilla_nav_warm_scan(mesh.raw.as_ptr(), reference, input.as_ptr(), &mut scan)
            };
            if let Some(expected_squared) = expected_squared {
                assert_eq!(status, SUCCESS, "{label}");
                let distance = f32::from_bits(expected_squared);
                if distance < minimum {
                    minimum = distance;
                }
            } else {
                assert_ne!(status & (1 << 31), 0, "{label}");
            }
            assert_eq!(
                scan.minimum_squared.to_bits(),
                minimum.to_bits(),
                "running original squared minimum {label}"
            );
            if scan.minimum_squared < 1. {
                break;
            }
        }
        let threshold = row.hex("max_distance");
        let selected = if scan.minimum_squared <= f32::from_bits(threshold) {
            scan.reference
        } else {
            0
        };
        assert_eq!(
            selected,
            row.get("selected_ref").parse::<u64>().unwrap(),
            "{label}"
        );
        assert_eq!(scan.distance.to_bits(), row.hex("distance"), "{label}");
        thresholds.insert(threshold);
        matches += 1;
    }
    assert!(
        matches >= 10,
        "only {matches} loaded source scan comparisons"
    );
    assert!(thresholds.contains(&3f32.to_bits()));
    assert!(thresholds.contains(&7f32.to_bits()));
}

fn lookup_primitive(value: &str) -> SourceWarmPrimitive {
    match value {
        "active" => SourceWarmPrimitive::Active,
        "tail" => SourceWarmPrimitive::Tail,
        "nearest5" => SourceWarmPrimitive::Nearest5,
        "nearest10" => SourceWarmPrimitive::Nearest10,
        other => panic!("unknown original lookup primitive {other}"),
    }
}

#[test]
fn source_lookup_calls_match_original_native_query_order_status_and_running_distance_bits() {
    let traces = oracle_rows(include_str!(
        "../tools/fixtures/warm-cache/traces.test-fixture"
    ));
    let mut cases_matched = 0;
    let mut primitives_matched = 0;
    for row in cases() {
        if row.count("limit") != 74
            || row.get("phase") == "excluded"
            || (row.get("direct_status") != "-" && row.hex("direct_status") != SUCCESS)
        {
            continue;
        }
        let label = row.get("label");
        let mut mesh = oracle_mesh(row.get("geometry"));
        let before = WarmCache::authored_fixture(
            &mesh,
            &row.refs("before_storage"),
            row.count("before_active"),
        )
        .unwrap();
        let result = mesh
            .loaded_source_warm_cache(
                &before,
                row.point("from"),
                row.point("to"),
                SourceWarmCacheBudget::default(),
                until(),
            )
            .unwrap();
        let expected: Vec<_> = traces
            .iter()
            .filter(|trace| trace.get("label") == label)
            .map(|trace| SourceWarmTrace {
                endpoint: trace.get("endpoint").parse().unwrap(),
                primitive: lookup_primitive(trace.get("primitive")),
                polygon: trace.get("polygon").parse().unwrap(),
                status: trace.hex("status"),
                distance_bits: trace.hex("distance_bits"),
            })
            .collect();
        assert!(!expected.is_empty(), "no actual source trace for {label}");
        let actual: Vec<_> = result
            .trace()
            .iter()
            .copied()
            .filter(|trace| trace.endpoint < 2)
            .collect();
        assert_eq!(actual, expected, "actual loaded primitive trace {label}");
        assert!(
            actual
                .windows(2)
                .all(|calls| calls[0].endpoint <= calls[1].endpoint),
            "end lookup ran before start completed: {label}"
        );
        primitives_matched += actual.len();
        cases_matched += 1;
    }
    assert!(
        cases_matched >= 12 && primitives_matched >= 30,
        "{cases_matched} cases, {primitives_matched} actual primitive calls"
    );
}

#[test]
fn authored_fixture_rejects_invalid_shapes_refs_and_oversized_operational_caps() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 0.);
    let to = point(-400., -100., 0.);
    let cache = seed(&mut mesh, from, to);
    let before = snapshot(&cache);
    for (storage, active) in [(vec![0; 73], 0), (vec![0; 4097], 0), (vec![0; 74], 75)] {
        assert_eq!(
            WarmCache::authored_fixture(&mesh, &storage, active).unwrap_err(),
            Error::Format
        );
    }
    for at in [0, 50, 74] {
        let mut storage = vec![0; 75];
        storage[at] = u64::MAX;
        assert_eq!(
            WarmCache::authored_fixture(&mesh, &storage, 1).unwrap_err(),
            Error::Identity
        );
    }
    for budget in [
        SourceWarmCacheBudget {
            lookup_primitives: SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES + 1,
            ..SourceWarmCacheBudget::default()
        },
        SourceWarmCacheBudget {
            corridor_expansions: SOURCE_WARM_CACHE_MAX_CORRIDOR_EXPANSIONS + 1,
            ..SourceWarmCacheBudget::default()
        },
    ] {
        assert_eq!(
            mesh.loaded_source_warm_cache(&cache, from, to, budget, until())
                .unwrap_err(),
            SourceWarmCacheError::Boundary(Error::Budget)
        );
        assert_eq!(snapshot(&cache), before);
    }
    let copy = cache.clone();
    assert_eq!(snapshot(&copy), before);
    assert_eq!(
        mesh.loaded_source_warm_cache(&copy, from, to, SourceWarmCacheBudget::default(), until())
            .unwrap()
            .phase(),
        SourceWarmPolyPhase::Cut
    );
    assert_eq!(snapshot(&cache), before);
}

#[test]
fn full_storage_append_budget_rejects_after_real_scans_and_retry_preserves_cache() {
    let mut mesh = existing_mesh();
    let a = point(-400., -400., 0.);
    let b = point(-400., -100., 0.);
    let reference = mesh.nearest(a, 5., until()).unwrap().polygon;
    let before =
        WarmCache::authored_fixture(&mesh, &vec![reference; SOURCE_WARM_CACHE_MAX_STORAGE], 1)
            .unwrap();
    let saved = snapshot(&before);
    let legacy = mesh.route(a, b, until()).unwrap();
    assert_eq!(
        mesh.loaded_source_warm_cache(&before, b, b, SourceWarmCacheBudget::default(), until())
            .unwrap_err(),
        SourceWarmCacheError::Boundary(Error::Budget)
    );
    assert_eq!(snapshot(&before), saved);
    assert_eq!(mesh.route(a, b, until()).unwrap(), legacy);
    let valid = mesh
        .loaded_source_warm_cache(&before, a, a, SourceWarmCacheBudget::default(), until())
        .unwrap();
    assert_eq!(valid.phase(), SourceWarmPolyPhase::SamePolygon);
    assert_eq!(snapshot(valid.after()), saved);
    assert_eq!(snapshot(&before), saved);
}

#[test]
fn inactive_slots_do_not_initialize_cold_distance_but_failed_nonempty_scans_do() {
    let mut mesh = existing_mesh();
    let point = point(-400., -400., 0.);
    let reference = mesh.nearest(point, 5., until()).unwrap().polygon;
    let inactive = WarmCache::authored_fixture(&mesh, &vec![reference; 74], 0).unwrap();
    assert_eq!(
        mesh.loaded_source_warm_cache(
            &inactive,
            point,
            point,
            SourceWarmCacheBudget::default(),
            until()
        )
        .unwrap_err(),
        SourceWarmCacheError::ColdDistanceUndefined
    );
    let failed_active = WarmCache::authored_fixture(&mesh, &[0; 74], 1).unwrap();
    let result = mesh
        .loaded_source_warm_cache(
            &failed_active,
            point,
            point,
            SourceWarmCacheBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(result.start().origin, SourceWarmLookupOrigin::Nearest5);
    assert!(result.start().appended);
    assert_eq!(result.end().origin, SourceWarmLookupOrigin::Tail);
    assert!(!result.end().appended);
    assert_eq!(result.start().polygon, reference);
    assert_eq!(result.end().polygon, reference);
    assert_eq!(result.after().storage().len(), 75);
    assert_eq!(result.after().storage()[74], reference);
    assert_eq!(result.after().active_len(), 1);
    assert_eq!(snapshot(&failed_active), (vec![0; 74], 1, 74, GROUND));
}

#[test]
fn seed_mint_keeps_the_original_corridor_deadline() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 0.);
    let to = point(-400., -100., 0.);
    let deadline = Instant::now() + Duration::from_secs(1);
    let corridor = mesh
        .loaded_source_corridor(
            from,
            to,
            GROUND,
            74,
            SourceSmoothingBudget::default(),
            deadline,
        )
        .unwrap();
    assert!(corridor.warm_cache_seed().is_ok());
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    assert_eq!(corridor.warm_cache_seed().unwrap_err(), Error::Deadline);
    drop(corridor);
    let cache = seed(&mut mesh, from, to);
    assert!(mesh
        .loaded_source_warm_cache(&cache, from, to, SourceWarmCacheBudget::default(), until())
        .is_ok());
}

#[test]
fn full_source_lookup_thresholds_tail_overwrite_append_and_real_nearest_fallback_match() {
    let mut labels = BTreeSet::new();
    let mut matched = 0;
    for row in oracle_rows(include_str!(
        "../tools/fixtures/warm-cache/lookups.test-fixture"
    )) {
        let label = row.get("label");
        assert!(
            labels.insert(label.to_owned()),
            "duplicate source lookup {label}"
        );
        for (suffix, offset) in [("below", -1i64), ("at", 0), ("above", 1)] {
            if label == format!("far_distance7_{suffix}") {
                assert_eq!(
                    row.hex("distance"),
                    (i64::from(7f32.to_bits()) + offset) as u32,
                    "exact original far-distance witness {label}"
                );
            }
        }
        if row.count("limit") != 74 {
            continue;
        }
        let mut mesh = oracle_mesh(row.get("geometry"));
        let before = WarmCache::authored_fixture(
            &mesh,
            &row.refs("before_storage"),
            row.count("before_active"),
        )
        .unwrap();
        let saved = snapshot(&before);
        let input = row.point("point");
        let result = mesh.loaded_source_warm_cache(
            &before,
            input,
            input,
            SourceWarmCacheBudget::default(),
            until(),
        );
        assert_eq!(snapshot(&before), saved, "lookup mutated input {label}");
        if before.active_len() == 0 && before.storage().len() == 74 {
            assert_eq!(
                result.unwrap_err(),
                SourceWarmCacheError::ColdDistanceUndefined,
                "{label}"
            );
            continue;
        }
        if row.get("selected") == "0" {
            assert_eq!(
                result.unwrap_err(),
                SourceWarmCacheError::MissingPolygon,
                "{label}"
            );
            continue;
        }
        if f32::from_bits(row.hex("distance")) > 7. {
            assert_eq!(
                result.unwrap_err(),
                SourceWarmCacheError::FarFromPolygonUnsupported,
                "{label}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        let selected = row.get("selected").parse().unwrap();
        assert_eq!(
            result.start(),
            SourceWarmLookup {
                polygon: selected,
                distance_bits: row.hex("distance"),
                origin: origin(row.get("origin")),
                appended: row.get("appended") == "1",
            },
            "full loaded lookup {label}"
        );
        assert_eq!(result.phase(), SourceWarmPolyPhase::SamePolygon, "{label}");
        assert_eq!(result.end().polygon, selected, "{label}");
        assert!(
            !result.end().appended,
            "unchanged repeated point appended twice: {label}"
        );
        let mut after = row.refs("after_storage");
        after[0] = selected;
        assert_eq!(
            result.after().storage(),
            after,
            "lookup tail survived same-polygon clear {label}"
        );
        assert_eq!(result.after().active_len(), 1);
        let calls: Vec<_> = result
            .trace()
            .iter()
            .filter(|call| call.endpoint == 0)
            .collect();
        for (primitive, field) in [
            (SourceWarmPrimitive::Active, "active_distance"),
            (SourceWarmPrimitive::Tail, "tail_distance"),
        ] {
            let last = calls.iter().rfind(|call| call.primitive == primitive);
            let present = if primitive == SourceWarmPrimitive::Active {
                before.active_len() > 0
            } else {
                row.get("tail_checked") == "1"
            };
            assert_eq!(last.is_some(), present, "lookup scan eligibility {label}");
            if let Some(last) = last {
                assert_eq!(
                    last.distance_bits,
                    row.hex(field),
                    "running scan distance {label}"
                );
            }
        }
        for (primitive, prefix) in [
            (SourceWarmPrimitive::Nearest5, "near5"),
            (SourceWarmPrimitive::Nearest10, "near10"),
        ] {
            let found = calls.iter().find(|call| call.primitive == primitive);
            assert_eq!(
                found.is_some(),
                row.get(&format!("{prefix}_checked")) == "1",
                "lookup fallback eligibility {label}"
            );
            if let Some(found) = found {
                assert_eq!(
                    found.status,
                    row.hex(&format!("{prefix}_status")),
                    "real nearest status {label}"
                );
                assert_eq!(
                    found.polygon,
                    row.get(&format!("{prefix}_ref")).parse::<u64>().unwrap(),
                    "real nearest reference {label}"
                );
            }
        }
        matched += 1;
    }
    assert!(matched >= 15, "only {matched} actual source lookup cases");
    for label in [
        "squared3_below",
        "squared3_at",
        "squared3_above",
        "squared7_below",
        "squared7_at",
        "squared7_above",
        "nearest_equal_no_append",
        "tail_overwrites_prevents_append",
        "tail_overwrites_appends_duplicate",
        "nearest10_fallback",
        "inactive_gap_not_scanned",
        "far_distance7_below",
        "far_distance7_at",
        "far_distance7_above",
    ] {
        assert!(
            labels.contains(label),
            "missing required source lookup {label}"
        );
    }
}

#[test]
fn end_lookup_failure_discards_a_real_start_append_and_retries_without_contamination() {
    let mut mesh = existing_mesh();
    let from = point(-400., -400., 0.);
    let old = point(-400., -100., 0.);
    let before = seed(&mut mesh, old, old);
    let saved = snapshot(&before);
    let legacy = mesh.route(from, old, until()).unwrap();
    let baseline = mesh
        .loaded_source_warm_cache(
            &before,
            from,
            from,
            SourceWarmCacheBudget::default(),
            until(),
        )
        .unwrap();
    assert!(baseline.start().appended);
    assert_eq!(baseline.end().origin, SourceWarmLookupOrigin::Tail);
    assert_eq!(baseline.after().storage().len(), 75);
    let missing = point(-400., 900., 0.);
    assert_eq!(mesh.nearest(missing, 10., until()), Err(Error::Unavailable));
    assert_eq!(
        mesh.loaded_source_warm_cache(
            &before,
            from,
            missing,
            SourceWarmCacheBudget::default(),
            until()
        )
        .unwrap_err(),
        SourceWarmCacheError::MissingPolygon
    );
    assert_eq!(snapshot(&before), saved);
    assert_eq!(mesh.route(from, old, until()).unwrap(), legacy);
    assert_eq!(
        mesh.loaded_source_warm_cache(
            &before,
            from,
            from,
            SourceWarmCacheBudget::default(),
            until()
        )
        .unwrap(),
        baseline
    );
    assert_eq!(snapshot(&before), saved);
}
