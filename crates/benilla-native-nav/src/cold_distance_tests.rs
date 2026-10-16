// SPDX-License-Identifier: GPL-3.0-or-later
//! Compatibility evidence is deliberately separate from unchanged-original warm fixtures.
use super::*;
use std::{collections::BTreeSet, time::Duration};

fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn loaded_mesh(geometry: &str) -> Mesh {
    let params = if geometry == "existing" {
        include_bytes!("../tools/fixtures/authored.mmap").as_slice()
    } else {
        include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture").as_slice()
    };
    let tiles: Vec<([u8; 2], &[u8])> = match geometry {
        "precision" => vec![(
            [31, 31],
            include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
        )],
        "existing" => vec![
            ([32, 32], include_bytes!("../tools/fixtures/0003232.mmtile")),
            ([32, 31], include_bytes!("../tools/fixtures/0003231.mmtile")),
        ],
        "grid" => vec![
            (
                [32, 32],
                include_bytes!("../tools/fixtures/direct-corridor/grid00.test-fixture"),
            ),
            (
                [32, 31],
                include_bytes!("../tools/fixtures/direct-corridor/grid10.test-fixture"),
            ),
            (
                [31, 32],
                include_bytes!("../tools/fixtures/direct-corridor/grid01.test-fixture"),
            ),
            (
                [31, 31],
                include_bytes!("../tools/fixtures/direct-corridor/grid11.test-fixture"),
            ),
        ],
        "long600" => vec![(
            [32, 32],
            include_bytes!("../tools/fixtures/direct-corridor/long600.test-fixture"),
        )],
        "long1100" => vec![(
            [32, 32],
            include_bytes!("../tools/fixtures/direct-corridor/long1100.test-fixture"),
        )],
        other => panic!("unexpected fixture {other}"),
    };
    let tiles: Vec<_> = tiles
        .into_iter()
        .map(|(grid, bytes)| TileInput { grid, bytes })
        .collect();
    Mesh::load(params, &tiles).unwrap()
}
struct Row {
    headings: Vec<&'static str>,
    values: Vec<&'static str>,
}
impl Row {
    fn get(&self, name: &str) -> &str {
        self.values[self
            .headings
            .iter()
            .position(|heading| *heading == name)
            .unwrap()]
    }
    fn count(&self, name: &str) -> usize {
        self.get(name).parse().unwrap()
    }
    fn word(&self, name: &str) -> u32 {
        u32::from_str_radix(self.get(name), 16).unwrap()
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
        self.get(name)
            .split(',')
            .map(|v| v.parse().unwrap())
            .collect()
    }
}
fn rows(text: &'static str) -> Vec<Row> {
    let headings: Vec<_> = text
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .unwrap();
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            assert_eq!(values.len(), headings.len());
            Row {
                headings: headings.clone(),
                values,
            }
        })
        .collect()
}
fn query(
    mesh: &mut Mesh,
    from: [u32; 3],
    to: [u32; 3],
) -> Result<LoadedSourceColdDistanceCompatibility, SourceWarmCacheError> {
    mesh.loaded_source_cold_distance_compatibility(
        SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
        from,
        to,
        SourceWarmCacheBudget::default(),
        until(),
    )
}
fn primitive(name: &str) -> SourceWarmPrimitive {
    match name {
        "active" => SourceWarmPrimitive::Active,
        "tail" => SourceWarmPrimitive::Tail,
        "nearest5" => SourceWarmPrimitive::Nearest5,
        "nearest10" => SourceWarmPrimitive::Nearest10,
        other => panic!("unexpected lookup primitive {other}"),
    }
}
#[test]
fn repaired_loaded_oracle_matches_bits_full_storage_order_and_retains_original_rejections() {
    let traces = rows(include_str!(
        "../tools/fixtures/cold-distance/traces.test-fixture"
    ));
    let mut seen = BTreeSet::new();
    let mut accepted = 0;
    for row in rows(include_str!(
        "../tools/fixtures/cold-distance/cases.test-fixture"
    )) {
        let label = row.get("label");
        assert!(seen.insert(label.to_owned()));
        assert_eq!(row.get("oracle"), "compatibility-zero-v1");
        let mut mesh = loaded_mesh(row.get("geometry"));
        let result = query(&mut mesh, row.point("from"), row.point("to"));
        let rejected = match row.get("phase") {
            "missing" => Some(SourceWarmCacheError::MissingPolygon),
            "far" => Some(SourceWarmCacheError::FarFromPolygonUnsupported),
            _ if row.word("direct_status") != 0 && row.word("direct_status") != SUCCESS => Some(
                SourceWarmCacheError::UnsupportedDirectStatus(row.word("direct_status")),
            ),
            _ => None,
        };
        if let Some(error) = rejected {
            assert_eq!(result.unwrap_err(), error, "{label}");
            assert_eq!(
                query(&mut mesh, row.point("from"), row.point("to")).unwrap_err(),
                error,
                "retry {label}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        accepted += 1;
        assert_eq!(result.check_mesh(&mesh), Ok(()));
        assert_eq!(
            result.compatibility_id(),
            "cmangos-buildpolypath-zero-distance-v1"
        );
        assert_eq!(result.compatibility_version(), 1);
        assert_eq!(
            result.source_revision(),
            "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
        );
        assert_eq!(result.initial_storage(), &[0; 74]);
        assert_eq!(result.initial_active_len(), 0);
        assert_eq!(result.point_limit(), 74);
        assert_eq!(
            result.filter(),
            SourceOwnerFilter {
                include_flags: 1,
                exclude_flags: 0
            }
        );
        assert_eq!(result.from(), row.point("from"));
        assert_eq!(result.to(), row.point("to"));
        assert_eq!(
            result.start().polygon,
            row.get("start_ref").parse::<u64>().unwrap(),
            "{label}"
        );
        assert_eq!(
            result.start().distance_bits,
            row.word("start_distance"),
            "{label}"
        );
        assert_eq!(
            result.end().polygon,
            row.get("end_ref").parse::<u64>().unwrap(),
            "{label}"
        );
        assert_eq!(
            result.end().distance_bits,
            row.word("end_distance"),
            "{label}"
        );
        assert!(result.start().appended, "cold zero disjunct {label}");
        assert_eq!(result.storage(), row.refs("after_storage"), "{label}");
        assert_eq!(result.active_len(), row.count("after_active"), "{label}");
        assert_eq!(
            result.phase(),
            if row.get("phase") == "same" {
                SourceWarmPolyPhase::SamePolygon
            } else {
                SourceWarmPolyPhase::Replacement
            },
            "{label}"
        );
        assert_eq!(
            result.direct_status(),
            (row.word("direct_status") != 0).then(|| row.word("direct_status")),
            "{label}"
        );
        let expected: Vec<_> = traces
            .iter()
            .filter(|trace| trace.get("label") == label)
            .map(|trace| SourceWarmTrace {
                endpoint: trace.count("endpoint") as u8,
                primitive: primitive(trace.get("primitive")),
                polygon: trace.get("polygon").parse().unwrap(),
                status: trace.word("status"),
                distance_bits: trace.word("distance_bits"),
            })
            .collect();
        let actual: Vec<_> = result
            .trace()
            .iter()
            .filter(|trace| trace.endpoint < 2)
            .copied()
            .collect();
        assert_eq!(actual, expected, "primitive calls {label}");
        if label == "cold_same_tail" {
            assert_eq!(result.start().origin, SourceWarmLookupOrigin::Nearest5);
            assert_eq!(result.end().origin, SourceWarmLookupOrigin::Tail);
            assert!(!result.end().appended);
            assert_eq!(result.storage().len(), 75);
        }
        assert_eq!(
            query(&mut mesh, row.point("from"), row.point("to")).unwrap(),
            result,
            "repeat {label}"
        );
    }
    assert_eq!(seen.len(), 14);
    assert_eq!(accepted, 6);
}
#[test]
fn explicit_version_mesh_identity_constructor_shape_and_original_cold_refusal_are_bound() {
    let mut mesh = loaded_mesh("precision");
    let from = point(5., 5., 0.);
    let to = point(5., 15., 0.);
    for version in [0, 2, u32::MAX] {
        assert_eq!(
            mesh.loaded_source_cold_distance_compatibility(
                version,
                from,
                to,
                SourceWarmCacheBudget::default(),
                until()
            )
            .unwrap_err(),
            SourceWarmCacheError::Boundary(Error::Unsupported)
        );
    }
    let original = WarmCache::authored_fixture(&mesh, &[0; 74], 0).unwrap();
    assert_eq!(
        mesh.loaded_source_warm_cache(
            &original,
            from,
            to,
            SourceWarmCacheBudget::default(),
            until()
        )
        .unwrap_err(),
        SourceWarmCacheError::ColdDistanceUndefined
    );
    let result = query(&mut mesh, from, to).unwrap();
    assert_eq!(original.storage(), &[0; 74]);
    assert_eq!(original.active_len(), 0);
    let other = loaded_mesh("precision");
    assert_eq!(result.check_mesh(&other), Err(Error::Identity));
    drop(mesh);
    let replacement = loaded_mesh("precision");
    assert_eq!(result.check_mesh(&replacement), Err(Error::Identity));
}
#[test]
fn compatibility_zero_does_not_change_any_original_warm_observation_or_rejection() {
    let mut covered = 0;
    for row in rows(include_str!(
        "../tools/fixtures/warm-cache/cases.test-fixture"
    )) {
        if row.count("limit") != 74
            || (row.count("before_active") == 0 && row.refs("before_storage").len() <= 74)
        {
            continue;
        }
        let mut mesh = loaded_mesh(row.get("geometry"));
        let before = WarmCache::authored_fixture(
            &mesh,
            &row.refs("before_storage"),
            row.count("before_active"),
        )
        .unwrap();
        let original = mesh.loaded_source_warm_cache(
            &before,
            row.point("from"),
            row.point("to"),
            SourceWarmCacheBudget::default(),
            until(),
        );
        let repaired = warm_cache::native::query_with_initialization(
            &mut mesh,
            &before,
            row.point("from"),
            row.point("to"),
            SourceWarmCacheBudget::default(),
            until(),
            warm_cache::DistanceInitialization::CompatibilityZeroV1,
        );
        assert_eq!(repaired, original, "warm behavior {}", row.get("label"));
        covered += 1;
    }
    assert!(covered >= 20);
}
#[test]
fn invalid_input_deadline_and_operational_budgets_do_not_poison_loaded_mesh() {
    let mut mesh = loaded_mesh("precision");
    let from = point(5., 5., 0.);
    let to = point(5., 15., 0.);
    let expected = query(&mut mesh, from, to).unwrap();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 100_001.] {
        assert_eq!(
            query(&mut mesh, point(value, 5., 0.), to).unwrap_err(),
            SourceWarmCacheError::Boundary(Error::Format)
        );
        assert_eq!(query(&mut mesh, from, to).unwrap(), expected);
    }
    assert_eq!(
        mesh.loaded_source_cold_distance_compatibility(
            1,
            from,
            to,
            SourceWarmCacheBudget::default(),
            Instant::now()
        )
        .unwrap_err(),
        SourceWarmCacheError::Boundary(Error::Deadline)
    );
    for budget in [
        SourceWarmCacheBudget {
            lookup_primitives: 0,
            corridor_expansions: 4096,
        },
        SourceWarmCacheBudget {
            lookup_primitives: 16388,
            corridor_expansions: 0,
        },
        SourceWarmCacheBudget {
            lookup_primitives: 16389,
            corridor_expansions: 4096,
        },
        SourceWarmCacheBudget {
            lookup_primitives: 16388,
            corridor_expansions: 4097,
        },
    ] {
        assert_eq!(
            mesh.loaded_source_cold_distance_compatibility(1, from, to, budget, until())
                .unwrap_err(),
            SourceWarmCacheError::Boundary(Error::Budget)
        );
        assert_eq!(query(&mut mesh, from, to).unwrap(), expected);
    }
}
