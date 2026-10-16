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
