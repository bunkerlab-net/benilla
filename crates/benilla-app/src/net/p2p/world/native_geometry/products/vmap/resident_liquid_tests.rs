// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine shared MPQ captures -> exact resident NO_WATER point observations.
//! These integration checks do not create a new liquid or location oracle.
use super::super::super::source_height::{
    CompositionError, PointNoWaterReply, PointNoWaterRequest, PreparedSourceHeight,
};
use super::super::prepared::PrepareError;
use super::source_height_tests::shared_entries;
use super::*;
use benilla_native_terrain::{self as terrain_source, PreparedTerrain};
use std::time::Duration;

const SELECTED: terrain_source::Tile = terrain_source::Tile { x: 31, y: 30 };
const AREA: &str = "DBFilesClient\\AreaTable.dbc";
const OTHER_ADT: &str = "World\\Maps\\Selected\\Selected_31_31.adt";
// Existing original public-query coordinates for the captured Hull geometry.
const HIT_XY: [u32; 2] = [0x43e8_d540, 0x4471_aaa0];
const MISS_XY: [u32; 2] = [0x43e7_d540, 0x4470_aaa0];

fn terrain_fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures")
            .join(name),
    )
    .unwrap()
}

pub(super) fn placed_terrain(name: &str) -> Vec<u8> {
    let mut adt = terrain_fixture(name);
    // Keep original MCIN/cell bytes; only add the established authored placement.
    for (field, tag, payload) in [
        (20, b"MWMO", format!("{ROOT}\0").into_bytes()),
        (32, b"MODF", fixture("hull.modf.test-fixture")),
    ] {
        let offset = u32::try_from(adt.len() - 20).unwrap();
        adt[20 + field..24 + field].copy_from_slice(&offset.to_le_bytes());
        adt.extend(chunk(tag, &payload));
    }
    adt
}

fn replace(entries: &mut [Entry], name: &str, bytes: Vec<u8>) {
    *entries.iter_mut().find(|entry| entry.name == name).unwrap() = raw_entry(name, bytes);
}

fn terrain(install: &Install) -> PreparedTerrain {
    terrain_source::prepare(&install.0, 0, &[SELECTED]).unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn request(view: &PreparedSourceHeight<'_, '_, '_>, z: f32, mask: u8) -> PointNoWaterRequest {
    PointNoWaterRequest {
        binding: view.binding(),
        ticket: 501,
        position: [HIT_XY[0], HIT_XY[1], z.to_bits()],
        mask,
    }
}

fn assert_validation(result: Result<PointNoWaterReply, CompositionError>, expected: Error) {
    match result {
        Err(CompositionError::Validation(actual)) => assert_eq!(actual, expected),
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}

fn assert_changed(error: CompositionError, member: &str) {
    match member {
        ROOT => assert!(matches!(
            error,
            CompositionError::Collision(PrepareError::Source(MapBundleError::Source(
                SourceError::Changed
            )))
        )),
        _ => assert!(matches!(error, CompositionError::TerrainSource)),
    }
}

#[test]
fn resident_liquid_dry_points_keep_exact_request_and_source_phase_order() {
    let install = Install::new(&shared_entries());
    let before = install.bytes();
    let terrain = terrain(&install);
    assert_eq!(
        terrain.tiles().next().unwrap().1,
        terrain_fixture("flat.map")
    );
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    // inventory() verifies all three assembled products against the original
    // extractor/assembler bytes. The established complete location consumer is
    // a comparison for resident composition, not a newly invented source oracle.
    let (spec, files) = inventory(&bundle, &capture.source_epoch());
    let complete = product_tests::load(spec, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap();
    for mask in [15, 10] {
        for (xy, z, expected_height, has_location) in [
            (HIT_XY, 8.0, 7.25f32, true),
            (HIT_XY, 6.0, 5.0, true),
            (HIT_XY, 20.0, 7.25, false),
            (MISS_XY, 8.0, 7.25, false),
        ] {
            let mut query = request(&view, z, mask);
            query.position[..2].copy_from_slice(&xy);
            let mut trace = Vec::new();
            let reply = view
                .point_no_water_observed(query, deadline(), || false, |phase| trace.push(phase))
                .unwrap();
            assert_eq!(trace, [0, 1, 2], "height, VMap location, then GridMap");
            assert_eq!(reply.request(), query);
            // These scalar values are already pinned by source_height_tests
            // against the original static-height selector and public ray oracle.
            assert_eq!(reply.initial_static_height(), expected_height.to_bits());
            let location = complete
                .query_location(
                    LocationRequest {
                        snapshot: complete.location_binding(),
                        ticket: query.ticket,
                        position: query.position,
                    },
                    deadline(),
                )
                .unwrap()
                .result;
            let expected = match location {
                StaticLocation::None => None,
                StaticLocation::Floor(floor) => Some(floor),
            };
            assert_eq!(reply.location(), expected);
            assert_eq!(reply.location().is_some(), has_location);
            assert_eq!(
                reply.effective_floor(),
                expected.map_or(reply.initial_static_height(), |location| location.floor)
            );
            if z == 8.0 && xy == HIT_XY {
                assert_ne!(reply.effective_floor(), reply.initial_static_height());
            }
            assert_eq!(
                view.point_no_water(query, deadline(), || false).unwrap(),
                reply
            );
        }
    }
    assert_eq!(
        install.bytes(),
        before,
        "point observation never writes source"
    );
}

#[test]
fn resident_liquid_present_terrain_liquid_is_not_point_or_mask_absence() {
    let mut entries = shared_entries();
    replace(
        &mut entries,
        ADT,
        placed_terrain("liquid_water_flat.adt.test-fixture"),
    );
    let install = Install::new(&entries);
    let terrain = terrain(&install);
    assert_eq!(
        terrain.tiles().next().unwrap().1,
        terrain_fixture("liquid_water_flat.map")
    );
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    for mask in [15, 10] {
        let mut trace = Vec::new();
        assert_validation(
            view.point_no_water_observed(
                request(&view, 20.0, mask),
                deadline(),
                || false,
                |phase| {
                    trace.push(phase);
                },
            ),
            Error::Unsupported,
        );
        assert_eq!(trace, [0, 1, 2], "terrain refusal follows height and VMap");
    }
}

pub(super) fn liquid_group_entries() -> Vec<Entry> {
    let mut entries = shared_entries();
    let wet = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../benilla-native-geometry-tests/tools/fixtures/wmo_liquid_source/inputs/Ocean_000.group.test-fixture",
        ),
    )
    .unwrap();
    // Reuse an unchanged original-source fixture's physical MLIQ chunk. This
    // new authored combination tests refusal, never liquid interpolation values.
    let offset = wet.windows(4).position(|tag| tag == b"QILM").unwrap();
    let size = u32::from_le_bytes(wet[offset + 4..offset + 8].try_into().unwrap()) as usize;
    let mut group = fixture("hull_000.group.test-fixture");
    group.extend_from_slice(&wet[offset..offset + 8 + size]);
    replace(&mut entries, GROUP, group);
    entries
}

#[test]
fn resident_liquid_selected_wet_group_refuses_before_terrain_fallback() {
    let install = Install::new(&liquid_group_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    assert_eq!(capture.summary().wmo_liquid_groups, 1);
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    for mask in [15, 10] {
        let mut trace = Vec::new();
        assert_validation(
            view.point_no_water_observed(
                request(&view, 8.0, mask),
                deadline(),
                || false,
                |phase| {
                    trace.push(phase);
                },
            ),
            Error::Unsupported,
        );
        assert_eq!(
            trace,
            [0, 1],
            "a wet selected group cannot fall through as dry"
        );
        // The same resident wet group grants no map-wide absence claim. Outside
        // its location bound, a separate exact point can have no selected group.
        let query = request(&view, 20.0, mask);
        let reply = view.point_no_water(query, deadline(), || false).unwrap();
        assert_eq!(reply.request(), query);
        assert_eq!(reply.location(), None);
        assert_eq!(reply.effective_floor(), reply.initial_static_height());
    }
}

#[test]
fn resident_liquid_missing_terrain_and_required_unloaded_collision_remain_missing() {
    let mut entries = shared_entries();
    // Preserve real terrain selection while moving its sole WMO residency to a
    // different tile. The full map index still describes the original bounds.
    replace(&mut entries, ADT, terrain_fixture("flat.adt.test-fixture"));
    let mut other = chunk(b"MVER", &18u32.to_le_bytes());
    other.extend(chunk(b"MWMO", format!("{ROOT}\0").as_bytes()));
    other.extend(chunk(b"MODF", &fixture("hull.modf.test-fixture")));
    entries.push(raw_entry(OTHER_ADT, other));
    let install = Install::new(&entries);
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(bundle.map().tiles()[0].coordinates(), (31, 31));
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let original = request(&view, 8.0, 15);
    for swapped in [false, true] {
        let mut query = original;
        if swapped {
            query.position.swap(0, 1); // no selected terrain at transposed grid
        }
        let mut trace = Vec::new();
        assert_validation(
            view.point_no_water_observed(query, deadline(), || false, |phase| trace.push(phase)),
            Error::Missing,
        );
        assert_eq!(trace, [0], "missing initial height cannot become NO_WATER");
    }
    // Primitive-level LOS coverage: this ray reaches the unavailable Hull leaf.
    // This does not claim a resident prefix can pass its earlier point filter.
    assert!(matches!(
        view.chase_ray(
            503,
            [HIT_XY[0], HIT_XY[1], 8.0f32.to_bits()],
            [HIT_XY[0], HIT_XY[1], 0.0f32.to_bits()],
            deadline(),
            || false,
        ),
        Err(CompositionError::Validation(Error::Missing))
    ));
    view.verify_unchanged(|| false).unwrap();
}

#[test]
fn resident_liquid_identity_mask_and_position_validation_never_publish_absence() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let original = request(&view, 8.0, 15);
    for field in 0..3 {
        let mut query = original;
        match field {
            0 => query.binding.map += 1,
            1 => query.binding.revision[0] ^= 1,
            _ => query.ticket = 0,
        }
        let mut trace = Vec::new();
        assert_validation(
            view.point_no_water_observed(query, deadline(), || false, |phase| trace.push(phase)),
            Error::Identity,
        );
        assert!(trace.is_empty());
    }
    for mask in [0, 1, 2, 8, 14, 16, u8::MAX] {
        assert_validation(
            view.point_no_water(PointNoWaterRequest { mask, ..original }, deadline(), || {
                false
            }),
            Error::Unsupported,
        );
    }
    for axis in 0..3 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut query = original;
            query.position[axis] = value.to_bits();
            assert_validation(
                view.point_no_water(query, deadline(), || false),
                Error::Format,
            );
        }
        for value in [-100_001.0f32, 100_001.0] {
            let mut query = original;
            query.position[axis] = value.to_bits();
            assert_validation(
                view.point_no_water(query, deadline(), || false),
                Error::Unsupported,
            );
        }
    }
}

#[test]
fn resident_liquid_cancellation_covers_phase_boundaries_and_expired_deadline() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let query = request(&view, 8.0, 15);
    let mut trace = Vec::new();
    assert_validation(
        view.point_no_water_observed(query, Instant::now(), || false, |phase| trace.push(phase)),
        Error::Deadline,
    );
    assert!(trace.is_empty());
    assert!(matches!(
        view.point_no_water_observed(query, deadline(), || true, |phase| trace.push(phase)),
        Err(CompositionError::Cancelled)
    ));
    assert!(trace.is_empty());
    for stop in 0..3 {
        let cancelled = Cell::new(false);
        let mut trace = Vec::new();
        let result = view.point_no_water_observed(
            query,
            deadline(),
            || cancelled.get(),
            |phase| {
                trace.push(phase);
                if phase == stop {
                    cancelled.set(true);
                }
            },
        );
        assert!(
            matches!(result, Err(CompositionError::Cancelled)),
            "{result:?}"
        );
        assert_eq!(trace, (0..=stop).collect::<Vec<_>>());
    }
}

#[test]
fn resident_liquid_source_mutation_before_or_during_observation_cannot_publish_reply() {
    for member in [AREA, ROOT, ADT] {
        let mut entries = shared_entries();
        let install = Install::new(&entries);
        let terrain = terrain(&install);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        let query = request(&view, 8.0, 15);
        let bytes = &mut entries
            .iter_mut()
            .find(|entry| entry.name == member)
            .unwrap()
            .bytes;
        let offset = match member {
            AREA => 24, // unused terrain-only AreaTable field
            ROOT => 48, // unused collision-only root ambient color
            ADT => {
                let cell = u32::from_le_bytes(bytes[92..96].try_into().unwrap()) as usize;
                cell + 8 + 4 // shared MCNK metadata; MCIN still owns placement
            }
            _ => unreachable!(),
        };
        bytes[offset] ^= 1;
        install.write(&entries);
        let changed = install.bytes();
        let mut trace = Vec::new();
        let result =
            view.point_no_water_observed(query, deadline(), || false, |phase| trace.push(phase));
        assert_changed(result.unwrap_err(), member);
        assert!(
            trace.is_empty(),
            "stale source is rejected before phase zero"
        );
        assert_eq!(install.bytes(), changed);

        // Restore exactly the receipt-bound bytes, then change them only after
        // both geometry phases. The final source recheck must reject the reply.
        install.write(&shared_entries());
        view.verify_unchanged(|| false).unwrap();
        let mut trace = Vec::new();
        let result = view.point_no_water_observed(
            query,
            deadline(),
            || false,
            |phase| {
                trace.push(phase);
                if phase == 2 {
                    install.write(&entries);
                }
            },
        );
        assert_changed(result.unwrap_err(), member);
        assert_eq!(trace, [0, 1, 2]);
        assert_eq!(
            install.bytes(),
            changed,
            "observation leaves the changed input untouched"
        );
    }
}

#[test]
fn resident_liquid_shared_budget_keeps_repeated_height_and_los_cumulative() {
    use super::super::super::source_height::HeightRequest;
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let outer = HeightRequest {
        binding: view.binding(),
        ticket: 701,
        position: [HIT_XY[0], HIT_XY[1], 20.0f32.to_bits()],
        max_search: 50.0f32.to_bits(),
    };
    let mut measured = QueryBudget::new(deadline());
    let initial = measured.remaining_work_for_test();
    let ground = view.query_with_budget(outer, &mut measured).unwrap();
    let height_cost = initial - measured.remaining_work_for_test();
    assert!(height_cost > 0);
    let mut nested = request(&view, f32::from_bits(ground.height), 15);
    nested.ticket = 702;
    assert_ne!(outer.position, nested.position);
    let mut phases = Vec::new();
    let reply = view
        .point_no_water_observed_with_budget(
            nested,
            &mut measured,
            || false,
            |phase, budget| phases.push((phase, initial - budget.remaining_work_for_test())),
        )
        .unwrap();
    let point_cost = initial - measured.remaining_work_for_test() - height_cost;
    assert_eq!(phases.iter().map(|p| p.0).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(phases[0].1, height_cost);
    assert!(
        phases[1].1 > phases[0].1,
        "nested static height executes again"
    );
    assert!(
        phases[2].1 > phases[1].1,
        "VMap selection consumes remaining work"
    );
    assert_eq!(
        initial - measured.remaining_work_for_test(),
        phases[2].1 + 1
    );
    assert_eq!(
        view.point_no_water(nested, deadline(), || false).unwrap(),
        reply
    );
    // Even a zero-length LOS is independently charged, not a free final phase.
    view.source_ray_with_budget(703, nested.position, nested.position, &mut measured, || {
        false
    })
    .unwrap();
    let total = initial - measured.remaining_work_for_test();
    assert!(total > height_cost + point_cost);
    for limit in [total, total - 1, height_cost, phases[1].1, phases[2].1] {
        let mut budget = QueryBudget::new(deadline());
        budget.restrict_work_for_test(limit);
        view.query_with_budget(outer, &mut budget).unwrap();
        let mut trace = Vec::new();
        let point = view.point_no_water_observed_with_budget(
            nested,
            &mut budget,
            || false,
            |phase, _| trace.push(phase),
        );
        if limit < height_cost + point_cost {
            assert_validation(point, Error::Budget);
            let last = if limit == height_cost {
                0
            } else if limit == phases[1].1 {
                1
            } else {
                2
            };
            assert_eq!(trace, (0..=last).collect::<Vec<_>>());
        } else {
            assert_eq!(point.unwrap(), reply);
            let ray = view.source_ray_with_budget(
                703,
                nested.position,
                nested.position,
                &mut budget,
                || false,
            );
            if limit == total {
                ray.unwrap();
                budget.complete().unwrap();
            } else {
                assert!(matches!(
                    ray,
                    Err(CompositionError::Validation(Error::Budget))
                ));
            }
        }
        assert_eq!(budget.remaining_work_for_test(), 0);
    }
    // Each phase separately fits total-minus-one; only cumulative accounting refuses.
    let mut separate = QueryBudget::new(deadline());
    separate.restrict_work_for_test(total - 1);
    view.point_no_water_with_budget(nested, &mut separate, || false)
        .unwrap();
    let mut separate = QueryBudget::new(deadline());
    separate.restrict_work_for_test(total - 1);
    view.source_ray_with_budget(703, nested.position, nested.position, &mut separate, || {
        false
    })
    .unwrap();
}

#[test]
fn resident_liquid_shared_budget_precedes_missing_terrain_and_wet_gridmap_when_exhausted() {
    for wet in [false, true] {
        let mut entries = shared_entries();
        if wet {
            replace(
                &mut entries,
                ADT,
                placed_terrain("liquid_water_flat.adt.test-fixture"),
            );
        }
        let install = Install::new(&entries);
        let terrain = terrain(&install);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        let mut query = request(&view, 20.0, 15);
        if !wet {
            query.position.swap(0, 1);
        }
        let mut budget = QueryBudget::new(deadline());
        let initial = budget.remaining_work_for_test();
        let mut grid_cost = 0;
        assert_validation(
            view.point_no_water_observed_with_budget(
                query,
                &mut budget,
                || false,
                |phase, budget| {
                    if phase == 2 {
                        grid_cost = initial - budget.remaining_work_for_test();
                    }
                },
            ),
            if wet {
                Error::Unsupported
            } else {
                Error::Missing
            },
        );
        let mut budget = QueryBudget::new(deadline());
        budget.restrict_work_for_test(grid_cost);
        assert_validation(
            view.point_no_water_with_budget(query, &mut budget, || false),
            Error::Budget,
        );
        assert_eq!(budget.remaining_work_for_test(), 0);
    }
}

#[test]
fn resident_liquid_borrowed_budget_never_renews_deadline_or_escapes_cancellation() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let query = request(&view, 8.0, 15);
    for stop in 0..3 {
        for expire in [false, true] {
            let end = deadline();
            let mut budget = QueryBudget::new(end);
            let cancelled = Cell::new(false);
            let mut trace = Vec::new();
            let result = view.point_no_water_observed_with_budget(
                query,
                &mut budget,
                || cancelled.get(),
                |phase, _| {
                    trace.push(phase);
                    if phase == stop {
                        if expire {
                            std::thread::sleep(end.saturating_duration_since(Instant::now()));
                        } else {
                            cancelled.set(true);
                        }
                    }
                },
            );
            if expire {
                assert_validation(result, Error::Deadline);
            } else {
                assert!(matches!(result, Err(CompositionError::Cancelled)));
            }
            assert_eq!(trace, (0..=stop).collect::<Vec<_>>());
        }
    }
    let mut budget = QueryBudget::new(Instant::now());
    let initial = budget.remaining_work_for_test();
    assert_validation(
        view.point_no_water_with_budget(query, &mut budget, || false),
        Error::Deadline,
    );
    assert_eq!(budget.remaining_work_for_test(), initial);
}
