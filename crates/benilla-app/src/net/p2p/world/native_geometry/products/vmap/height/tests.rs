// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::tests::snapshot;
use super::*;
use std::time::Duration;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn request(snapshot: &VmapSnapshot, internal: [f32; 3], search: f32) -> HeightRequest {
    HeightRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: to_internal(internal).map(f32::to_bits),
        max_search: search.to_bits(),
    }
}
fn bits(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}

#[test]
fn loaded_authored_heights_match_original_cpp_methods_bit_for_bit() {
    let snapshot = snapshot();
    let mut count = 0;
    let mut hits = 0;
    let mut rounded_inputs = 0;
    for line in include_str!(
        "../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedheight.tsv"
    )
    .lines()
    {
        if line.starts_with('#') {
            continue;
        }
        let row: Vec<_> = line.split('\t').collect();
        let query = HeightRequest {
            snapshot: snapshot.binding(),
            ticket: count + 1,
            position: [bits(row[1]), bits(row[2]), bits(row[3])],
            max_search: bits(row[4]),
        };
        let world = query.position.map(f32::from_bits);
        let internal = [bits(row[5]), bits(row[6]), bits(row[7])];
        assert_eq!(to_internal(world).map(f32::to_bits), internal, "{} conversion", row[0]);
        let reply = snapshot.query_height(query, deadline()).unwrap();
        assert_eq!(reply.request, query, "{} request", row[0]);
        assert_eq!(reply.height, bits(row[8]), "{} height", row[0]);
        hits += usize::from(reply.height != INVALID_HEIGHT.to_bits());
        rounded_inputs += usize::from(row[0].starts_with("rounded_world_"));
        count += 1;
    }
    assert_eq!(count, 1018);
    assert!(hits > 100);
    assert_eq!(rounded_inputs, 16);
}

#[test]
fn exact_vertical_height_keeps_strict_endpoints_m2_and_long_searches() {
    let snapshot = snapshot();
    for (position, search, expected) in [
        ([0.0, 0.0, 10.0], 10.0, INVALID_HEIGHT),
        ([0.0, 0.0, 10.0], 10.0f32.next_up(), 0.0),
        ([0.0, 0.0, -10.0], -10.0, INVALID_HEIGHT),
        ([0.0, 0.0, -10.0], -10.0f32.next_up(), 0.0),
        ([0.0, 0.0, 0.0], 20.0, INVALID_HEIGHT),
        ([20.0, 0.0, 6.0], 20.0, 3.0),
        ([20.0, 0.0, 6.0], -20.0, 9.0),
        ([100.0, -15.0, 10.0], 20.0, 2.0),
        ([100.0, -15.0, -10.0], -20.0, 2.0),
        ([20.0, 0.0, 9000.0], 10000.0, 9.0),
        ([20.0, 0.0, -9000.0], -10000.0, 0.0),
        ([0.0, 0.0, 10000.0], 10000.0, INVALID_HEIGHT),
        ([0.0, 0.0, 10.0], -0.0, INVALID_HEIGHT),
        ([0.0, 0.0, 10.0], 0.0, INVALID_HEIGHT),
    ] {
        let query = request(&snapshot, position, search);
        let reply = snapshot.query_height(query, deadline()).unwrap();
        assert_eq!(reply.request, query);
        assert_eq!(reply.height, expected.to_bits(), "{position:?} {search}");
    }
}

#[test]
fn height_binding_ticket_and_all_input_bits_are_preserved_or_rejected() {
    let snapshot = snapshot();
    let original = request(&snapshot, [0.0, 0.0, 10.0], -0.0);
    for field in 0..6 {
        let mut query = original;
        match field {
            0 => query.snapshot.map += 1,
            1 => query.snapshot.content = [0; 32],
            2 => query.snapshot.source_epoch = [0; 32],
            3 => query.snapshot.inventory = [0; 32],
            4 => query.snapshot.policy = [0; 32],
            _ => query.ticket = 0,
        }
        assert_eq!(
            snapshot.query_height(query, deadline()),
            Err(Error::Identity)
        );
    }
    assert_eq!(
        snapshot.query_height(original, deadline()).unwrap().request,
        original
    );
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for field in 0..4 {
            let mut query = original;
            if field == 3 {
                query.max_search = value.to_bits();
            } else {
                query.position[field] = value.to_bits();
            }
            assert_eq!(snapshot.query_height(query, deadline()), Err(Error::Format));
        }
    }
    for search in [MAX_SEARCH.next_up(), -MAX_SEARCH.next_up()] {
        let query = request(&snapshot, [0.0, 0.0, 10.0], search);
        assert_eq!(
            snapshot.query_height(query, deadline()),
            Err(Error::Unsupported)
        );
    }
    for axis in 0..3 {
        for value in [100_000.0f32.next_up(), -100_000.0f32.next_up()] {
            let mut query = original;
            query.position[axis] = value.to_bits();
            assert_eq!(
                snapshot.query_height(query, deadline()),
                Err(Error::Unsupported)
            );
        }
    }
}

#[test]
fn four_height_probes_share_work_and_an_absolute_deadline() {
    let snapshot = snapshot();
    let world = to_internal([0.0, 0.0, 10.0]);
    let mut budget = QueryBudget::new(deadline());
    let initial = budget.remaining;
    snapshot.height(world, 20.0, &mut budget).unwrap();
    let cost = initial - budget.remaining;
    assert!(cost > 1);
    budget.remaining = cost * 4 - 1;
    for _ in 0..3 {
        assert_eq!(snapshot.height(world, 20.0, &mut budget), Ok(0.0));
    }
    assert_eq!(
        snapshot.height(world, 20.0, &mut budget),
        Err(Error::Budget)
    );
    assert_eq!(budget.remaining, 0);
    // Finishing exactly at the work limit is valid; a subsequent ray is not.
    assert_eq!(budget.complete(), Ok(()));
    budget.deadline = Instant::now();
    assert_eq!(budget.complete(), Err(Error::Budget));
    assert_eq!(
        snapshot.height(world, 20.0, &mut budget),
        Err(Error::Budget)
    );
    assert_eq!(
        snapshot.query_height(request(&snapshot, [0.0, 0.0, 10.0], 20.0), Instant::now()),
        Err(Error::Budget)
    );
}

#[test]
fn unresolved_height_candidate_never_becomes_the_no_height_sentinel() {
    let mut snapshot = snapshot();
    snapshot.spawns[0] = None;
    let query = request(&snapshot, [0.0, 0.0, 10.0], 20.0);
    assert_eq!(
        snapshot.query_height(query, deadline()),
        Err(Error::Missing)
    );
}
