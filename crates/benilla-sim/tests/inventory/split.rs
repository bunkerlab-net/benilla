// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn state(rows: &[(u8, u32, u32)]) -> State {
    State {
        slots: rows
            .iter()
            .map(|&(slot, item, count)| (slot, instance(slot, item, count)))
            .collect(),
        equipped: BTreeMap::new(),
        next_item: 1000,
    }
}
#[test]
fn split_empty_destination_owns_clone_and_preserves_original_identity_and_count() {
    let c = catalog(20, 0);
    let mut s = state(&[(23, 70001, 10)]);
    let before = s.clone();
    let r = s.split_backpack(&c, 23, 24, 3).unwrap();
    r.validate(&c).unwrap();
    assert_eq!(r.outcome, SplitOutcome::Stored);
    assert_eq!(r.clone_id, Some(1000));
    assert_eq!(s.next_item, 1001);
    assert_eq!(
        (s.slots[&23].id, s.slots[&23].count),
        (before.slots[&23].id, 7)
    );
    assert_eq!((s.slots[&24].id, s.slots[&24].count), (1000, 3));
    assert_eq!(r.views.len(), 2);
    assert!(!r.views[0].slots.contains_key(&24));
    assert_eq!(s.item_count(70001).unwrap(), 10);
}
#[test]
fn split_partial_target_refills_source_and_retires_allocated_clone() {
    let c = catalog(10, 0);
    let mut s = state(&[(23, 70001, 9), (24, 70001, 8)]);
    let r = s.split_backpack(&c, 23, 24, 5).unwrap();
    assert_eq!(
        r.destinations,
        vec![
            Destination { slot: 24, count: 2 },
            Destination { slot: 23, count: 3 }
        ]
    );
    assert_eq!(
        (
            r.views[0].slots[&23].count,
            r.views[1].slots[&23].count,
            r.views[2].slots[&23].count
        ),
        (4, 4, 7)
    );
    assert_eq!((s.slots[&23].count, s.slots[&24].count), (7, 10));
    assert!(s.slots.values().all(|i| i.id != 1000));
    assert_eq!(s.next_item, 1001);
    r.validate(&c).unwrap();
}
#[test]
fn split_spill_visits_earlier_compatible_stack_before_original() {
    let c = catalog(10, 0);
    let mut s = state(&[(23, 70001, 6), (24, 70001, 8), (25, 70001, 9)]);
    let r = s.split_backpack(&c, 25, 24, 7).unwrap();
    assert_eq!(
        r.destinations,
        vec![
            Destination { slot: 24, count: 2 },
            Destination { slot: 23, count: 4 },
            Destination { slot: 25, count: 1 }
        ]
    );
    assert_eq!(
        (s.slots[&23].count, s.slots[&24].count, s.slots[&25].count),
        (10, 10, 3)
    );
    assert_eq!(s.item_count(70001).unwrap(), 23);
}
#[test]
fn split_guard_order_and_storage_refusal_allocation_are_distinct() {
    let c = catalog(10, 0);
    let original = state(&[(23, 70001, 8), (24, 70002, 1), (25, 70001, 10)]);
    for (src, dst, count, result, burn) in [
        (23, 23, 8, InventoryResult::Ok, false),
        (23, 24, 0, InventoryResult::Ok, false),
        (26, 24, 1, InventoryResult::ItemNotFound, false),
        (23, 24, 8, InventoryResult::CouldntSplitItems, false),
        (23, 24, 9, InventoryResult::TriedToSplitMoreThanCount, false),
        (23, 24, 2, InventoryResult::ItemCantStack, true),
        (23, 25, 2, InventoryResult::ItemCantStack, true),
    ] {
        let mut s = original.clone();
        let r = s.split_backpack(&c, src, dst, count).unwrap();
        assert_eq!(r.result, result);
        assert_eq!(s.slots, original.slots);
        assert_eq!(s.next_item, 1000 + u64::from(burn));
        assert!(r.views.is_empty());
        r.validate(&c).unwrap();
    }
}
#[test]
fn split_unique_limit_binding_and_template_metadata_remain_source_owned() {
    let mut row = item(70001, 20, 10);
    row.bonding = 1;
    row.maximum_durability = 37;
    let c = Catalog::new(vec![row], audit()).unwrap();
    let mut s = State::new(BTreeMap::new(), 1000).unwrap();
    s.store_new(&c, 70001, 10, |_, _, _| Ok(())).unwrap();
    let id = s.next_item;
    let r = s.split_backpack(&c, 23, 24, 3).unwrap();
    assert_eq!(s.item_count(70001).unwrap(), 10);
    assert_eq!(s.slots[&24].id, id);
    assert!(s.slots[&24].bound);
    assert_eq!(s.slots[&24].durability, 37);
    let mut bad = r.clone();
    bad.after.next_item += 1;
    assert!(bad.validate(&c).is_err());
    let mut bad = r.clone();
    bad.clone_id = Some(id + 1);
    assert!(bad.validate(&c).is_err());
    let mut bad = r.clone();
    bad.views[0].slots.get_mut(&23).unwrap().count += 1;
    assert!(bad.validate(&c).is_err());
    let mut bad = r;
    bad.destinations[0].count += 1;
    assert!(bad.validate(&c).is_err());
}
#[test]
fn split_exhaustion_and_unsupported_topology_leave_state_unchanged() {
    let c = catalog(10, 0);
    let mut s = state(&[(23, 70001, 8)]);
    s.next_item = u64::MAX;
    let before = s.clone();
    assert!(matches!(
        s.split_backpack(&c, 23, 24, 2),
        Err(Error::Exhausted)
    ));
    assert_eq!(s, before);
    assert!(matches!(
        s.split_backpack(&c, 23, 39, 2),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(s, before);
}
