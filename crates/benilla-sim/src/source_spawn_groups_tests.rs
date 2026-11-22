// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-mechanism fixtures, not recorded group-selection inputs.
use super::*;
#[derive(Default)]
struct Random {
    bounds: Vec<(u32, u32)>,
    shuffles: usize,
    roll: u32,
}
impl GroupRandom for Random {
    type Error = ();
    fn urand(&mut self, min: u32, max: u32) -> Result<u32, ()> {
        self.bounds.push((min, max));
        Ok(self.roll)
    }
    fn shuffle(&mut self, _: &mut [usize]) -> Result<(), ()> {
        self.shuffles += 1;
        Ok(())
    }
}
#[test]
fn explicit_entry_roll_is_cumulative_only_across_entries_with_remaining_capacity() {
    // SpawnGroup.cpp:129-141 subtracts a reached available entry's integer chance.
    let entries = [
        RandomEntry {
            entry: 10,
            min: 0,
            max: 1,
            chance: 40,
        },
        RandomEntry {
            entry: 20,
            min: 0,
            max: 1,
            chance: 40,
        },
    ];
    let mut valid = BTreeMap::from([(10, 1), (20, 1)]);
    let mut random = Random {
        roll: 75,
        ..Random::default()
    };
    assert_eq!(
        eligible_entry(
            &entries,
            &[0, 1],
            &[],
            &mut valid,
            &BTreeMap::new(),
            &mut random
        ),
        Ok(20)
    );
    assert_eq!(random.bounds, [(1, 100)]);
    assert_eq!(random.shuffles, 0);
}

#[test]
fn zero_capacity_is_unlimited_but_initial_group_count_uses_wrapped_entry_maxima() {
    // ObjectMgr.cpp:1469-1484; SpawnGroup.cpp:251-275.
    let entries = [RandomEntry {
        entry: 10,
        min: 2,
        max: 0,
        chance: 25,
    }];
    assert_eq!(default_max_count(0, &entries, 7), 0);
    let (valid, minimum) = entry_capacities(&entries, &BTreeMap::from([(7, 10)]));
    assert_eq!(valid[&10], u32::MAX - 1);
    assert_eq!(minimum[&10], 1);
    assert_eq!(
        default_max_count(
            0,
            &[RandomEntry {
                chance: 0,
                ..entries[0]
            }],
            7
        ),
        7
    );
    assert_eq!(default_max_count(0, &[], 7), 7);
}

fn member(ordinal: u64, slot: i32) -> GroupMember {
    GroupMember {
        group: 1,
        guid: ordinal as u32,
        slot,
        chance: 0,
        ordinal,
        own_entry: 0,
        random_entry: false,
    }
}
#[test]
fn equal_slot_members_keep_gcc12_partition_order_instead_of_a_stable_sort() {
    // Independent C++ source oracle: group-slot-sort-oracle.cpp; GCC 12.2 stl_algo.h:78-100,1780-1950.
    let mut members: Vec<_> = (0..20).map(|i| member(i, 0)).collect();
    sort_members(&mut members);
    assert_eq!(
        members.iter().map(|m| m.ordinal).collect::<Vec<_>>(),
        [10, 19, 18, 17, 16, 15, 14, 13, 12, 11, 0, 9, 8, 7, 6, 5, 4, 3, 2, 1]
    );
    let mut mixed: Vec<_> = (0..40).map(|i| member(i, (i % 5) as i32 - 2)).collect();
    sort_members(&mut mixed);
    assert_eq!(
        mixed.iter().map(|m| m.ordinal).collect::<Vec<_>>(),
        [
            0, 35, 5, 30, 25, 10, 20, 15, 1, 36, 31, 26, 21, 16, 11, 6, 12, 17, 37, 2, 22, 32, 7,
            27, 13, 38, 3, 33, 28, 8, 23, 18, 29, 4, 24, 34, 9, 19, 14, 39
        ]
    );
}

#[test]
fn minima_are_uniform_sorted_keys_and_capacity_exhaustion_skips_unavailable_chances() {
    let entries = [
        RandomEntry {
            entry: 10,
            min: 1,
            max: 1,
            chance: 40,
        },
        RandomEntry {
            entry: 20,
            min: 2,
            max: 1,
            chance: 40,
        },
    ];
    let mut valid = BTreeMap::from([(10, 0), (20, 1)]);
    let mut random = Random {
        roll: 1,
        ..Random::default()
    };
    assert_eq!(
        eligible_entry(
            &entries,
            &[0, 1],
            &[],
            &mut valid,
            &BTreeMap::from([(10, 1), (20, 2)]),
            &mut random
        ),
        Ok(20)
    );
    assert_eq!(random.bounds, [(0, 1)]);
    random.roll = 40;
    assert_eq!(
        eligible_entry(
            &entries,
            &[0, 1],
            &[],
            &mut valid,
            &BTreeMap::new(),
            &mut random
        ),
        Ok(20)
    );
    random.roll = 41;
    assert_eq!(
        eligible_entry(
            &entries,
            &[0, 1],
            &[0, 1],
            &mut valid,
            &BTreeMap::new(),
            &mut random
        ),
        Ok(20)
    );
    assert_eq!(random.shuffles, 1);
    valid.clear();
    assert_eq!(
        eligible_entry(
            &entries,
            &[],
            &[0, 1],
            &mut valid,
            &BTreeMap::new(),
            &mut random
        ),
        Ok(0)
    );
    assert_eq!(valid, BTreeMap::from([(10, 0), (20, 0)]));
    assert_eq!(
        eligible_entry(&[], &[99], &[99], &mut valid, &BTreeMap::new(), &mut random),
        Ok(0)
    );
}

#[test]
fn duplicate_entry_maxima_overwrite_but_minima_keep_first_insert_and_zero_entries_do_not_erase() {
    let entries = [
        RandomEntry {
            entry: 10,
            min: 1,
            max: 2,
            chance: 40,
        },
        RandomEntry {
            entry: 10,
            min: 8,
            max: 3,
            chance: 40,
        },
    ];
    let (mut valid, mut minimum) = entry_capacities(&entries, &BTreeMap::from([(7, 10), (8, 99)]));
    assert_eq!(valid, BTreeMap::from([(10, 2), (99, 0)]));
    assert!(minimum.is_empty());
    minimum.insert(10, 2);
    erase_entry(0, &mut valid, &mut minimum);
    erase_entry(10, &mut valid, &mut minimum);
    assert_eq!(valid[&10], 1);
    assert_eq!(minimum[&10], 1);
    erase_entry(10, &mut valid, &mut minimum);
    assert_eq!(valid[&10], 0);
    assert!(!minimum.contains_key(&10));
    erase_entry(99, &mut valid, &mut minimum);
    assert_eq!(valid[&99], 0);
    assert_eq!(default_max_count(8, &entries, 2), 8);
    assert_eq!(default_max_count(0, &entries, 2), 2);
    let wrapped = [
        RandomEntry {
            max: u32::MAX,
            ..entries[0]
        },
        RandomEntry {
            max: 2,
            ..entries[1]
        },
    ];
    assert_eq!(default_max_count(0, &wrapped, 9), 1);
}

#[test]
fn invalid_indices_signed_chance_overflow_and_random_owner_errors_are_distinct_refusals() {
    let entries = [RandomEntry {
        entry: 10,
        min: 0,
        max: 1,
        chance: 0x8000_0000,
    }];
    let mut valid = BTreeMap::from([(10, 1)]);
    let mut random = Random {
        roll: 1,
        ..Random::default()
    };
    assert_eq!(
        eligible_entry(
            &entries,
            &[0],
            &[],
            &mut valid,
            &BTreeMap::new(),
            &mut random
        ),
        Err(SelectionError::ChanceSubtractUndefined)
    );
    assert_eq!(
        eligible_entry(
            &entries,
            &[1],
            &[],
            &mut valid,
            &BTreeMap::new(),
            &mut random
        ),
        Err(SelectionError::EntryIndex)
    );
    assert_eq!(
        eligible_entry(
            &entries,
            &[],
            &[1],
            &mut valid,
            &BTreeMap::new(),
            &mut random
        ),
        Err(SelectionError::EntryIndex)
    );
    random.roll = 2;
    assert_eq!(
        eligible_entry(
            &entries,
            &[],
            &[],
            &mut valid,
            &BTreeMap::from([(10, 1)]),
            &mut random
        ),
        Err(SelectionError::RandomIndex)
    );
    struct Failed;
    impl GroupRandom for Failed {
        type Error = ();
        fn urand(&mut self, _: u32, _: u32) -> Result<u32, ()> {
            Err(())
        }
        fn shuffle(&mut self, _: &mut [usize]) -> Result<(), ()> {
            Err(())
        }
    }
    assert_eq!(
        eligible_entry(
            &entries,
            &[0],
            &[],
            &mut valid,
            &BTreeMap::new(),
            &mut Failed
        ),
        Err(SelectionError::Owner(()))
    );
    assert_eq!(
        eligible_entry(
            &entries,
            &[],
            &[],
            &mut valid,
            &BTreeMap::from([(10, 1)]),
            &mut Failed
        ),
        Err(SelectionError::Owner(()))
    );
    assert_eq!(
        eligible_entry(
            &entries,
            &[],
            &[0],
            &mut valid,
            &BTreeMap::new(),
            &mut Failed
        ),
        Err(SelectionError::Owner(()))
    );
}

#[test]
fn sorting_small_inputs_and_forced_heap_fallback_keep_ordered_slots() {
    sort_members(&mut []);
    for count in 1..80 {
        let mut members: Vec<_> = (0..count)
            .map(|i| member(i, (count - i) as i32 % 9))
            .collect();
        sort_members(&mut members);
        assert!(members.windows(2).all(|pair| pair[0].slot <= pair[1].slot));
        let mut heap: Vec<_> = (0..count).map(|i| member(i, (i % 7) as i32)).collect();
        if count > 1 {
            heap_sort(&mut heap);
        }
        assert!(heap.windows(2).all(|pair| pair[0].slot <= pair[1].slot));
    }
}

#[test]
fn vicinity_uses_strict_squared_distance_with_caller_z_and_negative_range() {
    let position = [0, 0, 0, 0];
    let spawn = [3_f32.to_bits(), 4_f32.to_bits(), f32::NAN.to_bits()];
    assert!(!in_vicinity(position, spawn, 5_f32.to_bits()));
    assert!(in_vicinity(position, spawn, 6_f32.to_bits()));
    assert!(in_vicinity(position, spawn, (-6_f32).to_bits()));
    assert!(!in_vicinity(
        [0, 0, f32::NAN.to_bits(), 0],
        spawn,
        6_f32.to_bits()
    ));
    assert!(!in_vicinity(position, spawn, f32::NAN.to_bits()));
    assert!(!in_vicinity(
        position,
        [f32::INFINITY.to_bits(), 0, 0],
        f32::INFINITY.to_bits()
    ));
}
