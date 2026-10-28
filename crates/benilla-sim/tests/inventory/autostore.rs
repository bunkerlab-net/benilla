// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored probes of pinned HandleAutoStoreBagItem/_CanStoreItem/StoreItem order.
use super::*;
fn state(rows: &[(u8, u32, u32)]) -> State {
    let mut state = State::new(BTreeMap::new(), 1000).unwrap();
    state.slots = rows
        .iter()
        .map(|&(slot, entry, count)| (slot, instance(slot, entry, count)))
        .collect();
    state
}
#[test]
fn absent_source_and_sole_original_slot_have_no_detached_or_store_views() {
    let c = catalog(5, 0);
    let mut s = state(&[(23, 70001, 3)]);
    let before = s.clone();
    let r = s.autostore_backpack(&c, 38).unwrap();
    assert_eq!(r.outcome, AutoStoreOutcome::IgnoredEmptySource);
    assert!(r.views.is_empty());
    assert!(r.destinations.is_empty());
    assert_eq!(s, before);
    let r = s.autostore_backpack(&c, 23).unwrap();
    assert_eq!(r.outcome, AutoStoreOutcome::Unchanged);
    assert_eq!(r.destinations, vec![Destination { slot: 23, count: 3 }]);
    assert!(r.views.is_empty());
    assert_eq!(s, before);
}
#[test]
fn earlier_empty_slot_reuses_original_instance_without_allocating() {
    let c = catalog(5, 0);
    let mut s = state(&[(38, 70001, 5)]);
    let original = s.slots[&38].clone();
    let r = s.autostore_backpack(&c, 38).unwrap();
    assert_eq!(r.outcome, AutoStoreOutcome::Stored);
    assert_eq!(r.destinations, vec![Destination { slot: 23, count: 5 }]);
    assert_eq!(r.views.len(), 2);
    assert!(r.views[0].slots.is_empty());
    assert_eq!(r.views[1], s);
    assert_eq!(s.slots[&23], original);
    assert_eq!(s.next_item, 1000);
}
#[test]
fn ordered_merges_exclude_source_then_reinsert_original_remainder() {
    let c = catalog(5, 0);
    let mut s = state(&[(23, 70001, 4), (24, 70001, 4), (25, 70001, 5)]);
    let mut remainder = s.slots[&25].clone();
    remainder.count = 3;
    let r = s.autostore_backpack(&c, 25).unwrap();
    assert_eq!(
        r.destinations,
        vec![
            Destination { slot: 23, count: 1 },
            Destination { slot: 24, count: 1 },
            Destination { slot: 25, count: 3 }
        ]
    );
    assert_eq!(r.views.len(), 4);
    assert!(!r.views[0].slots.contains_key(&25));
    assert_eq!(
        (r.views[0].slots[&23].count, r.views[0].slots[&24].count),
        (4, 4)
    );
    assert_eq!(
        (r.views[1].slots[&23].count, r.views[1].slots[&24].count),
        (5, 4)
    );
    assert_eq!(
        (r.views[2].slots[&23].count, r.views[2].slots[&24].count),
        (5, 5)
    );
    assert_eq!(s.slots[&25], remainder);
    assert!(r.views.iter().all(|v| v.next_item == 1000));
    assert_eq!(
        r.before.item_count(70001).unwrap(),
        s.item_count(70001).unwrap()
    );
}
#[test]
fn full_merge_removes_only_original_source_and_preserves_destination_ids() {
    let c = catalog(5, 0);
    let mut s = state(&[(23, 70001, 4), (24, 70001, 4), (38, 70001, 2)]);
    let ids = (s.slots[&23].id, s.slots[&24].id);
    let source_id = s.slots[&38].id;
    let r = s.autostore_backpack(&c, 38).unwrap();
    assert_eq!(
        r.destinations,
        vec![
            Destination { slot: 23, count: 1 },
            Destination { slot: 24, count: 1 }
        ]
    );
    assert_eq!(r.views.len(), 3);
    assert_eq!((s.slots[&23].id, s.slots[&24].id), ids);
    assert!(s.slots.values().all(|i| i.id != source_id));
    assert_eq!(s.next_item, 1000);
}
#[test]
fn full_backpack_uses_original_slot_capacity_without_false_inventory_full() {
    let c = catalog(5, 0);
    let mut rows: Vec<_> = (23..39).map(|slot| (slot, 70002, 1)).collect();
    rows[0] = (23, 70001, 4);
    rows[15] = (38, 70001, 5);
    let mut s = state(&rows);
    let id = s.slots[&38].id;
    s.autostore_backpack(&c, 38).unwrap();
    assert_eq!(s.slots.len(), 16);
    assert_eq!(
        (s.slots[&23].count, s.slots[&38].count, s.slots[&38].id),
        (5, 4, id)
    );
    assert_eq!(s.next_item, 1000);
}
#[test]
fn unsupported_position_or_storage_closure_is_atomic() {
    let c = catalog(5, 0);
    let mut s = state(&[(23, 70001, 5)]);
    let before = s.clone();
    for source in [0, 22, 39, 255] {
        assert!(s.autostore_backpack(&c, source).is_err());
        assert_eq!(s, before);
    }
    let mut absent = audit();
    absent.item_creation_and_loot_scripts = Evidence::Unknown;
    let c = Catalog::new(vec![item(70001, 5, 0)], absent).unwrap();
    assert!(s.autostore_backpack(&c, 23).is_err());
    assert_eq!(s, before);
}
#[test]
fn store_binding_and_unique_limit_use_existing_source_without_new_item() {
    let mut row = item(70001, 5, 9);
    row.bonding = 1;
    let c = Catalog::new(vec![row], audit()).unwrap();
    let mut unchanged = state(&[(23, 70001, 3)]);
    let before = unchanged.clone();
    let noop = unchanged.autostore_backpack(&c, 23).unwrap();
    assert_eq!(noop.outcome, AutoStoreOutcome::Unchanged);
    assert!(noop.views.is_empty());
    assert!(!unchanged.slots[&23].bound);
    assert_eq!(unchanged, before);
    noop.validate(&c).unwrap();
    let mut s = state(&[(23, 70001, 4), (38, 70001, 5)]);
    let id = s.slots[&38].id;
    let r = s.autostore_backpack(&c, 38).unwrap();
    assert!(s.slots.values().all(|i| i.bound));
    assert_eq!(s.slots[&24].id, id);
    assert_eq!(s.item_count(70001).unwrap(), 9);
    assert_eq!(s.next_item, 1000);
    r.validate(&c).unwrap();
    for fault in 0..4 {
        let mut bad = r.clone();
        match fault {
            0 => bad.after.next_item += 1,
            1 => bad.destinations.reverse(),
            2 => {
                bad.views.remove(0);
            }
            _ => bad.source = 23,
        }
        assert!(bad.validate(&c).is_err());
    }
}
