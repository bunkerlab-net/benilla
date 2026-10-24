// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::time::Duration;

const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const LEFT: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
fn query(slot: SourceMapQuerySlot) -> SourceMapQuery {
    SourceMapQuery::load(
        slot,
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: LEFT,
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

#[test]
fn bounded512_and_source1024_are_distinct_real_allocations() {
    let bounded = Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: LEFT,
        }],
    )
    .unwrap();
    let source = query(SLOT);
    assert_eq!(bounded.query_profile(), QueryProfile::Bounded512);
    assert_eq!(bounded.query_capacity(), 512);
    assert_eq!(source.query_profile(), QueryProfile::SourceMapInstance1024);
    assert_eq!(source.query_capacity(), 1024);
    assert_ne!(bounded.instance_id, source.inner.mesh.borrow().instance_id);
    assert_eq!(source.slot().instance_id, 0);
    assert_ne!(
        source.inner.mesh.borrow().instance_id,
        u64::from(source.slot().instance_id)
    );
    assert_eq!(bounded.agent_dimensions(), source.agent_dimensions());
}

#[test]
fn query1024_traverses_more_than_the_legacy512_node_pool() {
    let params = include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture");
    let tiles = [TileInput {
        grid: [32, 32],
        bytes: include_bytes!("../tools/fixtures/direct-corridor/long600.test-fixture"),
    }];
    let mut bounded = Mesh::load(params, &tiles).unwrap();
    let source = SourceMapQuery::load(SLOT, params, &tiles).unwrap();
    // Existing raw oracle identifies the endpoints; no new reference or corridor input.
    let row = include_str!("../tools/fixtures/direct-corridor/cases.test-fixture")
        .lines()
        .find(|line| line.starts_with("long600_1024\t"))
        .unwrap();
    let fields: Vec<_> = row.split('\t').collect();
    let parse = |text: &str| -> [u32; 3] {
        text.split(',')
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    };
    let from = parse(fields[2]);
    let to = parse(fields[3]);
    fn sliced_status(mesh: &mut Mesh, from: [u32; 3], to: [u32; 3]) -> (u32, usize) {
        let a = mesh.nearest(from, 5., until()).unwrap();
        let b = mesh.nearest(to, 5., until()).unwrap();
        let start = crate::detour(from).unwrap();
        let end = crate::detour(to).unwrap();
        // SAFETY: live validated owner and exact output widths, finalized before return.
        unsafe {
            let mut status = crate::ffi::benilla_nav_begin(
                mesh.raw.as_ptr(),
                a.polygon,
                b.polygon,
                start.as_ptr(),
                end.as_ptr(),
            );
            let mut iterations = 0;
            while status & crate::PROGRESS != 0 {
                let mut done = 0;
                status = crate::ffi::benilla_nav_step(mesh.raw.as_ptr(), &mut done);
                iterations += done as usize;
                assert!(iterations <= 1100);
            }
            let mut path = [0; 256];
            let mut count = 0;
            let result =
                crate::ffi::benilla_nav_finish(mesh.raw.as_ptr(), path.as_mut_ptr(), &mut count);
            assert!(count > 0);
            (result, iterations)
        }
    }
    let (legacy_status, legacy_steps) = sliced_status(&mut bounded, from, to);
    let (source_status, source_steps) =
        sliced_status(&mut source.inner.mesh.borrow_mut(), from, to);
    assert_ne!(legacy_status & (1 << 5), 0); // DT_OUT_OF_NODES
    assert_eq!(source_status & (1 << 5), 0);
    assert!(legacy_steps <= 512);
    assert_eq!(source_steps, 600);
    assert_ne!(source_status & (1 << 4), 0); // bounded output is still truncated
}

#[test]
fn source_slot_is_independent_of_allocation_identity() {
    let a = query(SLOT);
    let b = query(SLOT);
    let other_slot = query(SourceMapQuerySlot {
        instance_id: 7,
        ..SLOT
    });
    assert_eq!(a.slot(), b.slot());
    assert_eq!(a.inner.mesh.borrow().origin, b.inner.mesh.borrow().origin);
    let retained = a.retain_default();
    assert_eq!(retained.check_query(&a), Ok(()));
    assert_eq!(retained.check_query(&b), Err(Error::Identity));
    assert_eq!(retained.check_query(&other_slot), Err(Error::Identity));
    assert_eq!(other_slot.slot().instance_id, 7);
}

#[test]
fn canonical_reload_reexecutes_cache_but_tile_order_and_allocation_are_not_interchangeable() {
    const CONFIG: crate::SourceSamePolygonConfig = crate::SourceSamePolygonConfig {
        normalize_z: false,
        ignore_normalization: false,
    };
    fn load(reverse: bool) -> SourceMapQuery {
        // ProductId::MmapTile orders by grid x/y; the raw loader preserves caller order.
        let mut tiles = [
            TileInput {
                grid: [32, 31],
                bytes: include_bytes!("../tools/fixtures/0003231.mmtile"),
            },
            TileInput {
                grid: [32, 32],
                bytes: LEFT,
            },
        ];
        if reverse {
            tiles.reverse();
        }
        SourceMapQuery::load(SLOT, PARAMS, &tiles).unwrap()
    }
    fn observations_equal(a: &crate::LoadedSourceSamePolygon, b: &crate::LoadedSourceSamePolygon) {
        // Mesh IDs must differ; compare every public calculation observation instead.
        let (a_cache, b_cache) = (a.compatibility(), b.compatibility());
        assert_eq!(a_cache.compatibility_id(), b_cache.compatibility_id());
        assert_eq!(
            a_cache.compatibility_version(),
            b_cache.compatibility_version()
        );
        assert_eq!(a_cache.source_revision(), b_cache.source_revision());
        assert_eq!(a_cache.is_retained(), b_cache.is_retained());
        assert_eq!(a_cache.initial_storage(), b_cache.initial_storage());
        assert_eq!(a_cache.initial_active_len(), b_cache.initial_active_len());
        assert_eq!(a_cache.before_storage(), b_cache.before_storage());
        assert_eq!(a_cache.before_active_len(), b_cache.before_active_len());
        assert_eq!(a_cache.storage(), b_cache.storage());
        assert_eq!(a_cache.active_len(), b_cache.active_len());
        assert_eq!(a_cache.point_limit(), b_cache.point_limit());
        assert_eq!(a_cache.filter(), b_cache.filter());
        assert_eq!(a_cache.from(), b_cache.from());
        assert_eq!(a_cache.to(), b_cache.to());
        assert_eq!(a_cache.start(), b_cache.start());
        assert_eq!(a_cache.end(), b_cache.end());
        assert_eq!(a_cache.phase(), b_cache.phase());
        assert_eq!(a_cache.direct_status(), b_cache.direct_status());
        assert_eq!(a_cache.trace(), b_cache.trace());
        assert_eq!(a.config(), b.config());
        assert_eq!(a.points(), b.points());
        assert_eq!(a.actual_end(), b.actual_end());
        assert_eq!(a.path_type(), b.path_type());
        assert_eq!(a.terminal_primitives(), b.terminal_primitives());
        assert_eq!(a.slot(), b.slot());
        assert_eq!(a.query_capacity(), b.query_capacity());
    }
    let canonical = load(false);
    let reloaded = load(false);
    let reversed = load(true);
    assert_ne!(
        canonical.inner.mesh.borrow().instance_id,
        reloaded.inner.mesh.borrow().instance_id
    );
    let from = point(-400., -400., 0.);
    let to = point(-300., -300., 0.);
    let other_tile = point(-400., 100., 2.);
    let nearest = |query: &SourceMapQuery, position| {
        query
            .inner
            .mesh
            .borrow_mut()
            .nearest(position, 5., until())
            .unwrap()
    };
    let original = nearest(&canonical, from);
    assert_eq!(original, nearest(&reloaded, from));
    assert_ne!(original.polygon, nearest(&reversed, from).polygon);
    let relocated = nearest(&reversed, other_tile);
    // Both successful native lookups return a valid ref, now naming different tiles.
    assert!(original.over && relocated.over);
    assert_eq!(original.polygon, relocated.polygon);
    assert_ne!(original.point, relocated.point);
    assert_eq!(
        nearest(&canonical, other_tile).polygon,
        nearest(&reversed, from).polygon
    );

    let budget = crate::SourceSamePolygonBudget::default();
    let mut original_owner = canonical.retain_default();
    let mut fresh_owner = reloaded.retain_default();
    let initial = original_owner
        .select_for_calculate(SLOT, from, to)
        .unwrap()
        .loaded_cold_same_polygon(1, CONFIG, budget, until())
        .unwrap();
    let selected = fresh_owner.select_for_calculate(SLOT, from, to).unwrap();
    assert_eq!(initial.check_query(&selected), Err(Error::Identity));
    let fresh_initial = selected
        .loaded_cold_same_polygon(1, CONFIG, budget, until())
        .unwrap();
    observations_equal(&initial, &fresh_initial);
    let continued = original_owner
        .select_for_calculate(SLOT, to, from)
        .unwrap()
        .loaded_retained_same_polygon(&initial, CONFIG, budget, until())
        .unwrap();
    let selected = fresh_owner.select_for_calculate(SLOT, to, from).unwrap();
    assert_eq!(
        selected
            .loaded_retained_same_polygon(&initial, CONFIG, budget, until())
            .unwrap_err(),
        crate::SourceSamePolygonError::Boundary(Error::Identity)
    );
    let fresh_continued = selected
        .loaded_retained_same_polygon(&fresh_initial, CONFIG, budget, until())
        .unwrap();
    observations_equal(&continued, &fresh_continued);
    assert_eq!(fresh_continued.check_query(&selected), Ok(()));
}

#[test]
fn current_query_starts_unselected_and_first_valid_calculate_selects_it() {
    let query = query(SLOT);
    let mut retained = query.retain_default();
    assert!(!retained.current_selected());
    let from = point(-400., -400., 0.);
    let to = point(-300., -300., 0.);
    {
        let selected = retained.select_for_calculate(SLOT, from, to).unwrap();
        assert_eq!(selected.slot(), SLOT);
        assert_eq!(selected.query_capacity(), 1024);
        assert_eq!(selected.from(), from);
        assert_eq!(selected.to(), to);
        let presence = selected.tile_presence(until()).unwrap();
        assert!(presence.both_present());
        assert_eq!(selected.check_tile_presence(&presence), Ok(()));
    }
    assert!(retained.current_selected());
}

#[test]
fn invalid_coordinates_never_transition_the_current_query() {
    let query = query(SLOT);
    for selected_first in [false, true] {
        let mut retained = query.retain_default();
        let valid = point(-400., -400., 0.);
        if selected_first {
            retained.select_for_calculate(SLOT, valid, valid).unwrap();
        }
        let invalid = point(f32::NAN, 0., 0.);
        assert_eq!(
            retained
                .select_for_calculate(SLOT, invalid, invalid)
                .unwrap_err(),
            SourceQuerySelectionError::Coordinates(SourceMapCoordError::DestinationInvalid)
        );
        assert_eq!(retained.current_selected(), selected_first);
        assert_eq!(
            retained
                .select_for_calculate(SLOT, invalid, valid)
                .unwrap_err(),
            SourceQuerySelectionError::Coordinates(SourceMapCoordError::StartInvalid)
        );
        assert_eq!(retained.current_selected(), selected_first);
    }
}

#[test]
fn map_and_instance_change_are_rejected_without_transition() {
    let query = query(SLOT);
    let mut retained = query.retain_default();
    for slot in [
        SourceMapQuerySlot { map_id: 1, ..SLOT },
        SourceMapQuerySlot {
            instance_id: 1,
            ..SLOT
        },
    ] {
        assert_eq!(
            retained
                .select_for_calculate(slot, [0; 3], [0; 3])
                .unwrap_err(),
            SourceQuerySelectionError::Identity
        );
        assert!(!retained.current_selected());
    }
}

#[test]
fn source_valid_coordinates_select_before_later_native_budget_refusal() {
    let query = query(SLOT);
    let mut retained = query.retain_default();
    let point = point(0., 0., 100_001.);
    let selected = retained.select_for_calculate(SLOT, point, point).unwrap();
    assert_eq!(selected.tile_presence(until()), Err(Error::Unsupported));
    assert!(retained.current_selected());
}

#[test]
fn tile_evidence_cannot_transfer_between_equal_byte_queries_or_endpoints() {
    let query = query(SLOT);
    let duplicate = self::query(SLOT);
    let from = point(-400., -400., 0.);
    let to = point(-300., -300., 0.);
    let mut first = query.retain_default();
    let selected = first.select_for_calculate(SLOT, from, to).unwrap();
    let presence = selected.tile_presence(until()).unwrap();
    let mut second = duplicate.retain_default();
    let other = second.select_for_calculate(SLOT, from, to).unwrap();
    assert_eq!(other.check_tile_presence(&presence), Err(Error::Identity));
    let swapped = first.select_for_calculate(SLOT, to, from).unwrap();
    assert_eq!(swapped.check_tile_presence(&presence), Err(Error::Identity));
}

#[test]
fn source_loader_validates_every_input_before_native_allocation() {
    assert!(matches!(
        SourceMapQuery::load(SLOT, &[], &[]),
        Err(Error::Format | Error::Framing)
    ));
    for length in [0, 19, LEFT.len() - 1] {
        assert!(SourceMapQuery::load(
            SLOT,
            PARAMS,
            &[TileInput {
                grid: [32, 32],
                bytes: &LEFT[..length]
            }]
        )
        .is_err());
    }
}

#[test]
fn retained_default_keeps_the_same_native_allocation_alive() {
    let mut retained = {
        let source = query(SLOT);
        source.retain_default()
    };
    let p = point(-400., -400., 0.);
    let selected = retained.select_for_calculate(SLOT, p, p).unwrap();
    assert_eq!(selected.query_capacity(), 1024);
    assert!(selected.tile_presence(until()).unwrap().both_present());
}
