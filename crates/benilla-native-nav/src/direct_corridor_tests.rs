// SPDX-License-Identifier: GPL-3.0-or-later
//! The original direct findPath oracle runs on authored, source-written loaded meshes.
//! This private FFI harness obtains references from that validated mesh's nearest lookup;
//! no public corridor constructor accepts references, and no owner route is admitted here.
use super::*;
use std::{collections::BTreeSet, ffi::c_void, time::Duration};

const GROUND: SourceOwnerFilter = SourceOwnerFilter {
    include_flags: 1,
    exclude_flags: 0,
};
const BUFFER_TOO_SMALL: u32 = 1 << 4;
const OUT_OF_NODES: u32 = 1 << 5;
const PARTIAL_RESULT: u32 = 1 << 6;
const INVALID_PARAM: u32 = (1 << 31) | (1 << 3);

unsafe extern "C" {
    fn benilla_nav_source_nearest(
        mesh: *mut c_void,
        p: *const f32,
        e: *const f32,
        include: u16,
        exclude: u16,
        reference: *mut u64,
        nearest: *mut f32,
        over: *mut u8,
    ) -> u32;
    fn benilla_nav_source_direct_create(
        mesh: *mut c_void,
        start: u64,
        end: u64,
        start_pos: *const f32,
        end_pos: *const f32,
        include: u16,
        exclude: u16,
        status: *mut u32,
    ) -> *mut c_void;
    fn benilla_source_direct_step(state: *mut c_void, done: *mut i32) -> u32;
    fn benilla_source_direct_finish(
        state: *mut c_void,
        path: *mut u64,
        count: *mut i32,
        capacity: i32,
    ) -> u32;
    fn benilla_source_direct_stats(
        state: *const c_void,
        expansions: *mut u32,
        nodes: *mut u32,
        state_nodes: *mut u32,
    );
    fn benilla_source_direct_free(state: *mut c_void);
}

fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
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
            .unwrap()]
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
            .map(|value| u32::from_str_radix(value, 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
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
    fn nearest(&self, prefix: &str) -> Nearest {
        Nearest {
            polygon: self.get(&format!("{prefix}_ref")).parse().unwrap(),
            point: self.point(&format!("{prefix}_nearest")),
            over: match self.get(&format!("{prefix}_over")) {
                "0" => false,
                "1" => true,
                other => panic!("invalid oracle over flag {other}"),
            },
        }
    }
}

fn oracle_rows() -> Vec<OracleRow<'static>> {
    let fixture = include_str!("../tools/fixtures/direct-corridor/cases.test-fixture");
    let headings: Vec<_> = fixture
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .expect("missing original direct oracle schema");
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
    const PARAMS: &[u8] = include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture");
    let (params, tiles): (&[u8], Vec<_>) = match geometry {
        "existing" => (
            include_bytes!("../tools/fixtures/authored.mmap"),
            vec![
                TileInput {
                    grid: [32, 32],
                    bytes: include_bytes!("../tools/fixtures/0003232.mmtile"),
                },
                TileInput {
                    grid: [32, 31],
                    bytes: include_bytes!("../tools/fixtures/0003231.mmtile"),
                },
            ],
        ),
        "long600" => (
            PARAMS,
            vec![TileInput {
                grid: [32, 32],
                bytes: include_bytes!("../tools/fixtures/direct-corridor/long600.test-fixture"),
            }],
        ),
        "long1100" => (
            PARAMS,
            vec![TileInput {
                grid: [32, 32],
                bytes: include_bytes!("../tools/fixtures/direct-corridor/long1100.test-fixture"),
            }],
        ),
        "grid" => (
            PARAMS,
            vec![
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
        ),
        other => panic!("unknown authored direct geometry {other}"),
    };
    Mesh::load(params, &tiles).unwrap()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stats {
    expansions: u32,
    nodes: u32,
    state_nodes: u32,
}

/// Exclusive borrow keeps the validated native mesh alive through every state access.
/// References are obtained by lookup here, never supplied by the caller.
struct DirectSearch<'a> {
    raw: NonNull<c_void>,
    _mesh: &'a mut Mesh,
    status: u32,
    start: Nearest,
    end: Nearest,
}
impl<'a> DirectSearch<'a> {
    fn new(mesh: &'a mut Mesh, from: [u32; 3], to: [u32; 3]) -> Self {
        let a = detour(from).unwrap();
        let b = detour(to).unwrap();
        let lookup = |point: [f32; 3]| {
            let mut polygon = 0;
            let mut nearest = [0.; 3];
            let mut over = 0;
            // SAFETY: validated mesh, finite authored inputs, fixed initialized outputs.
            let status = unsafe {
                benilla_nav_source_nearest(
                    mesh.raw.as_ptr(),
                    point.as_ptr(),
                    [5.; 3].as_ptr(),
                    GROUND.include_flags,
                    GROUND.exclude_flags,
                    &mut polygon,
                    nearest.as_mut_ptr(),
                    &mut over,
                )
            };
            assert_eq!(status, SUCCESS);
            assert_ne!(polygon, 0);
            assert!(over <= 1);
            Nearest {
                polygon,
                point: world(nearest),
                over: over == 1,
            }
        };
        let start = lookup(a);
        let end = lookup(b);
        let mut status = 0;
        // SAFETY: references were just looked up on this validated mesh; positions are
        // finite, and the guard retains the mesh until the copied-input state is freed.
        let raw = unsafe {
            benilla_nav_source_direct_create(
                mesh.raw.as_ptr(),
                start.polygon,
                end.polygon,
                a.as_ptr(),
                b.as_ptr(),
                GROUND.include_flags,
                GROUND.exclude_flags,
                &mut status,
            )
        };
        let search = Self {
            raw: NonNull::new(raw).expect("create source direct state"),
            _mesh: mesh,
            status,
            start,
            end,
        };
        assert!(matches!(status, SUCCESS | PROGRESS));
        search
    }
    fn stats(&self) -> Stats {
        let mut stats = Stats::default();
        // SAFETY: live owned state and three initialized scalar output slots.
        unsafe {
            benilla_source_direct_stats(
                self.raw.as_ptr(),
                &mut stats.expansions,
                &mut stats.nodes,
                &mut stats.state_nodes,
            );
        }
        assert!(stats.nodes <= 1024);
        assert!(stats.state_nodes <= stats.nodes);
        stats
    }
    fn step(&mut self) -> u32 {
        let before = self.stats();
        let was_pending = self.status == PROGRESS;
        let mut done = 0;
        // SAFETY: live uniquely owned state and initialized scalar output.
        self.status = unsafe { benilla_source_direct_step(self.raw.as_ptr(), &mut done) };
        let after = self.stats();
        assert_eq!(done, i32::from(was_pending));
        assert_eq!(after.expansions, before.expansions + u32::from(was_pending));
        self.status
    }
    fn complete(&mut self) -> Stats {
        while self.status == PROGRESS {
            assert!(self.stats().expansions < 4096, "unbounded test search");
            self.step();
        }
        assert_eq!(self.status, SUCCESS);
        self.stats()
    }
    fn finish(&self, capacity: usize) -> (u32, Vec<u64>) {
        self.finish_with_limit(i32::try_from(capacity).unwrap())
    }
    fn finish_with_limit(&self, capacity: i32) -> (u32, Vec<u64>) {
        assert!((-1..=1100).contains(&capacity));
        let mut path = vec![0; capacity.max(1) as usize];
        let mut count = 0;
        // SAFETY: live state and initialized buffer covering every positive capacity;
        // nonpositive limits must be rejected before a write, with a valid spare slot.
        let status = unsafe {
            benilla_source_direct_finish(self.raw.as_ptr(), path.as_mut_ptr(), &mut count, capacity)
        };
        assert!((0..=capacity.max(0)).contains(&count));
        if status == INVALID_PARAM {
            assert_eq!(count, 0);
            assert!(path.iter().all(|reference| *reference == 0));
        }
        path.truncate(count as usize);
        (status, path)
    }
}
impl Drop for DirectSearch<'_> {
    fn drop(&mut self) {
        // SAFETY: sole state owner; the mesh borrow remains alive throughout destruction.
        unsafe { benilla_source_direct_free(self.raw.as_ptr()) }
    }
}

#[test]
fn raw_direct_matches_original_status_corridor_and_node_diagnostics() {
    let mut labels = BTreeSet::new();
    let mut statuses = BTreeSet::new();
    let mut matched = 0;
    let mut nonzero_states = 0;
    for row in oracle_rows() {
        let label = row.get("label");
        assert!(
            labels.insert(label.to_owned()),
            "duplicate oracle case {label}"
        );
        if row.count("nodes") != 1024 || !(1..=1024).contains(&row.count("capacity")) {
            continue;
        }
        let mut mesh = oracle_mesh(row.get("geometry"));
        let mut search = DirectSearch::new(&mut mesh, row.point("from"), row.point("to"));
        assert_eq!(row.hex("from_status"), SUCCESS, "{label}");
        assert_eq!(row.hex("to_status"), SUCCESS, "{label}");
        assert_eq!(search.start, row.nearest("from"), "{label}");
        assert_eq!(search.end, row.nearest("to"), "{label}");
        let stats = search.complete();
        let expected = (row.hex("direct_status"), row.refs("direct_corridor"));
        assert_eq!(search.finish(row.count("capacity")), expected, "{label}");
        assert_eq!(stats.nodes as usize, row.count("direct_nodes"), "{label}");
        assert_eq!(
            stats.state_nodes as usize,
            row.count("direct_state_nodes"),
            "{label}"
        );
        // Reconstruction and stepping an already completed search must not mutate it.
        assert_eq!(search.step(), SUCCESS, "{label}");
        assert_eq!(search.stats(), stats, "{label}");
        assert_eq!(search.finish(row.count("capacity")), expected, "{label}");
        statuses.insert(expected.0);
        nonzero_states += usize::from(stats.state_nodes != 0);
        matched += 1;
    }
    assert!(matched >= 19, "only {matched} original direct cases");
    assert!(
        nonzero_states >= 4,
        "cross-side state variants were not exercised"
    );
    for status in [
        SUCCESS,
        SUCCESS | BUFFER_TOO_SMALL,
        SUCCESS | PARTIAL_RESULT,
        SUCCESS | PARTIAL_RESULT | OUT_OF_NODES,
        SUCCESS | BUFFER_TOO_SMALL | PARTIAL_RESULT | OUT_OF_NODES,
    ] {
        assert!(
            statuses.contains(&status),
            "missing raw direct status {status:08x}"
        );
    }
}

#[test]
fn production_corridor_matches_complete_source_rows_and_rejects_every_detail_flag() {
    let mut accepted = 0;
    let mut rejected = 0;
    for row in oracle_rows() {
        if row.count("nodes") != 1024 || !(2..=74).contains(&row.count("capacity")) {
            continue;
        }
        let label = row.get("label");
        let mut mesh = oracle_mesh(row.get("geometry"));
        let result = mesh.loaded_source_corridor(
            row.point("from"),
            row.point("to"),
            GROUND,
            row.count("capacity"),
            SourceSmoothingBudget::default(),
            until(),
        );
        if row.hex("direct_status") == SUCCESS {
            let corridor = result.unwrap();
            assert_eq!(corridor.original_from(), row.point("from"), "{label}");
            assert_eq!(corridor.original_to(), row.point("to"), "{label}");
            assert_eq!(corridor.start(), row.nearest("from"), "{label}");
            assert_eq!(corridor.end(), row.nearest("to"), "{label}");
            assert_eq!(corridor.filter(), GROUND, "{label}");
            assert_eq!(corridor.point_limit(), row.count("capacity"), "{label}");
            assert_eq!(corridor.polygons(), row.refs("direct_corridor"), "{label}");
            accepted += 1;
        } else {
            let failure = result.expect_err("incomplete source status became a corridor");
            assert_eq!(
                failure.error,
                Error::Native(row.hex("direct_status")),
                "{label}"
            );
            assert_eq!(
                failure.diagnostics,
                SourceSmoothingDiagnostics::default(),
                "{label}"
            );
            rejected += 1;
        }
    }
    assert!(
        accepted >= 8 && rejected >= 4,
        "accepted {accepted}, rejected {rejected}"
    );
}

#[test]
fn exact_capacity_direct_success_and_cross_side_route_are_not_sliced_substitutes() {
    for row in oracle_rows().into_iter().filter(|row| {
        matches!(
            row.get("label"),
            "two_exact_capacity"
                | "cross_side_exact_capacity"
                | "seventyfour_exact_capacity"
                | "cross_side_route_diverges"
        )
    }) {
        let label = row.get("label");
        let expected = row.refs("direct_corridor");
        assert_eq!(row.hex("direct_status"), SUCCESS, "{label}");
        if label.ends_with("exact_capacity") {
            assert_eq!(expected.len(), row.count("capacity"), "{label}");
            assert_eq!(
                row.hex("sliced_status"),
                SUCCESS | BUFFER_TOO_SMALL,
                "{label}"
            );
        } else {
            assert_eq!(row.hex("sliced_status"), SUCCESS);
        }
        if row.get("geometry") == "grid" {
            assert_ne!(expected, row.refs("sliced_corridor"), "{label}");
            assert!(row.count("direct_state_nodes") > 0, "{label}");
            assert_ne!(
                row.count("direct_nodes"),
                row.count("sliced_nodes"),
                "{label}"
            );
        }
        let mut mesh = oracle_mesh(row.get("geometry"));
        let corridor = mesh
            .loaded_source_corridor(
                row.point("from"),
                row.point("to"),
                GROUND,
                row.count("capacity"),
                SourceSmoothingBudget::default(),
                until(),
            )
            .unwrap();
        assert_eq!(corridor.polygons(), expected, "{label}");
    }
}

#[test]
fn same_polygon_requires_no_expansion_and_exact_pop_budget_is_sufficient() {
    let mut zero_expansion = 0;
    let mut bounded = 0;
    for row in oracle_rows() {
        if row.count("nodes") != 1024
            || row.hex("direct_status") != SUCCESS
            || !(2..=74).contains(&row.count("capacity"))
        {
            continue;
        }
        let label = row.get("label");
        let mut mesh = oracle_mesh(row.get("geometry"));
        let mut search = DirectSearch::new(&mut mesh, row.point("from"), row.point("to"));
        let stats = search.complete();
        if search.start.polygon == search.end.polygon {
            assert_eq!(stats, Stats::default(), "{label}");
            zero_expansion += 1;
        } else {
            assert!(stats.expansions > 0, "{label}");
        }
        drop(search);
        let budget = SourceSmoothingBudget {
            corridor_expansions: stats.expansions as usize,
            smoothing_primitives: 0,
        };
        let corridor = mesh
            .loaded_source_corridor(
                row.point("from"),
                row.point("to"),
                GROUND,
                row.count("capacity"),
                budget,
                until(),
            )
            .unwrap();
        assert_eq!(corridor.polygons(), row.refs("direct_corridor"), "{label}");
        drop(corridor);
        if stats.expansions != 0 {
            let failure = mesh
                .loaded_source_corridor(
                    row.point("from"),
                    row.point("to"),
                    GROUND,
                    row.count("capacity"),
                    SourceSmoothingBudget {
                        corridor_expansions: budget.corridor_expansions - 1,
                        ..budget
                    },
                    until(),
                )
                .expect_err("one missing native pop still completed a corridor");
            assert_eq!(failure.error, Error::Budget, "{label}");
            assert_eq!(
                failure.diagnostics,
                SourceSmoothingDiagnostics::default(),
                "{label}"
            );
            bounded += 1;
        }
    }
    assert_eq!(zero_expansion, 1);
    assert!(bounded >= 7, "only {bounded} exact pop-budget cases");
}

#[test]
fn cancelling_private_direct_state_preserves_legacy_query_and_fresh_direct_reuse() {
    let rows = oracle_rows();
    let row = rows
        .iter()
        .find(|row| row.get("label") == "cross_side_route_diverges")
        .unwrap();
    let mut mesh = oracle_mesh(row.get("geometry"));
    let from = row.point("from");
    let to = row.point("to");
    let legacy = mesh.route(from, to, until()).unwrap();
    assert_eq!(legacy.polygons, row.refs("sliced_corridor"));
    let mut baseline = DirectSearch::new(&mut mesh, from, to);
    let stats = baseline.complete();
    assert!(stats.expansions >= 3);
    let direct = baseline.finish(row.count("capacity"));
    assert_eq!(direct, (SUCCESS, row.refs("direct_corridor")));
    assert_ne!(direct.1, legacy.polygons);
    drop(baseline);
    for pops in 0..stats.expansions {
        let mut search = DirectSearch::new(&mut mesh, from, to);
        for _ in 0..pops {
            assert_eq!(search.step(), PROGRESS);
        }
        let before = search.stats();
        assert_eq!(search.finish(74), (INVALID_PARAM, Vec::new()));
        assert_eq!(search.stats(), before);
        drop(search);
        assert_eq!(
            mesh.route(from, to, until()).unwrap(),
            legacy,
            "cancel after {pops}"
        );
        let mut retried = DirectSearch::new(&mut mesh, from, to);
        assert_eq!(retried.complete(), stats, "retry after {pops}");
        assert_eq!(retried.finish(74), direct, "retry after {pops}");
        drop(retried);
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
        assert_eq!(
            corridor.polygons(),
            direct.1,
            "production retry after {pops}"
        );
        drop(corridor);
        assert_eq!(
            mesh.route(from, to, until()).unwrap(),
            legacy,
            "reuse after {pops}"
        );
    }
}

#[test]
fn source_pool_has_1024_nodes_and_only_actual_exhaustion_sets_the_detail_flag() {
    let rows = oracle_rows();
    let row = |label| rows.iter().find(|row| row.get("label") == label).unwrap();
    let smaller_pool = row("long600_512");
    assert_eq!(smaller_pool.count("direct_nodes"), 512);
    assert_eq!(
        smaller_pool.hex("direct_status"),
        SUCCESS | BUFFER_TOO_SMALL | PARTIAL_RESULT | OUT_OF_NODES
    );
    for (label, nodes, status) in [
        ("long600_1024", 600, SUCCESS | BUFFER_TOO_SMALL),
        ("long600_full", 600, SUCCESS),
        ("long1100_exact_pool1024_full", 1024, SUCCESS),
        (
            "long1100_outnodes_capacity1024",
            1024,
            SUCCESS | PARTIAL_RESULT | OUT_OF_NODES,
        ),
        (
            "long1100_1024",
            1024,
            SUCCESS | BUFFER_TOO_SMALL | PARTIAL_RESULT | OUT_OF_NODES,
        ),
    ] {
        let row = row(label);
        assert_eq!(row.hex("direct_status"), status, "{label}");
        assert_eq!(row.count("direct_nodes"), nodes, "{label}");
        let mut mesh = oracle_mesh(row.get("geometry"));
        let mut search = DirectSearch::new(&mut mesh, row.point("from"), row.point("to"));
        let stats = search.complete();
        assert_eq!(stats.nodes as usize, nodes, "{label}");
        assert_eq!(
            search.finish(row.count("capacity")),
            (status, row.refs("direct_corridor")),
            "{label}"
        );
    }
}

#[test]
fn invalid_raw_capacity_rejects_without_poisoning_completed_state() {
    let rows = oracle_rows();
    let row = rows
        .iter()
        .find(|row| row.get("label") == "long600_full")
        .unwrap();
    let mut mesh = oracle_mesh(row.get("geometry"));
    let mut search = DirectSearch::new(&mut mesh, row.point("from"), row.point("to"));
    let stats = search.complete();
    let expected = (SUCCESS, row.refs("direct_corridor"));
    assert_eq!(search.finish(600), expected);
    for capacity in [-1, 0, 1025, 1100] {
        assert_eq!(
            search.finish_with_limit(capacity),
            (INVALID_PARAM, Vec::new())
        );
        assert_eq!(search.stats(), stats);
        assert_eq!(search.finish(600), expected);
    }
    drop(search);
    for capacity in [1, 75, 600, 1024] {
        let failure = mesh
            .loaded_source_corridor(
                row.point("from"),
                row.point("to"),
                GROUND,
                capacity,
                SourceSmoothingBudget::default(),
                until(),
            )
            .expect_err("private diagnostic capacity expanded public source admission");
        assert_eq!(failure.error, Error::Format);
        assert_eq!(failure.diagnostics, SourceSmoothingDiagnostics::default());
    }
}
