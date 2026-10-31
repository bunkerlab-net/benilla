// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn state() -> State {
    State {
        slots: [(23, instance(23, 70001, 5)), (24, instance(24, 70001, 3))].into(),
        equipped: BTreeMap::new(),
        next_item: 1000,
    }
}
#[test]
fn selected_destroy_is_planned_without_mutation_or_allocation_and_never_spills() {
    let c = catalog(5, 0);
    let s = state();
    for (count, remaining, removed) in [(0, 0, 3), (1, 2, 1), (3, 0, 3), (255, 0, 3)] {
        let r = s.plan_destroy_backpack(&c, 24, count).unwrap();
        r.validate(&c).unwrap();
        assert_eq!(r.before, s);
        assert_eq!(r.after.next_item, s.next_item);
        assert_eq!(r.after.slots[&23], s.slots[&23]);
        assert_eq!(r.after.slots.get(&24).map_or(0, |i| i.count), remaining);
        assert_eq!(r.removed, removed);
    }
    assert_eq!(s.slots[&24].count, 3);
}
#[test]
fn protected_prototype_not_ordinary_bound_instance_controls_refusal() {
    let mut row = item(70001, 5, 0);
    row.flags = NO_USER_DESTROY;
    let c = Catalog::new(vec![row], audit()).unwrap();
    let mut s = state();
    let r = s.plan_destroy_backpack(&c, 24, 0).unwrap();
    assert_eq!(r.result, InventoryResult::CantDropSoulbound);
    assert_eq!(r.result.protocol(), 24);
    assert_eq!(r.before, r.after);
    assert_eq!(r.removed, 0);
    r.validate(&c).unwrap();
    s.slots.get_mut(&24).unwrap().bound = true;
    let mut bound_row = item(70001, 5, 0);
    bound_row.bonding = 1;
    let bound_catalog = Catalog::new(vec![bound_row], audit()).unwrap();
    let r = s.plan_destroy_backpack(&bound_catalog, 24, 0).unwrap();
    assert_eq!(r.result, InventoryResult::Ok);
    assert!(!r.after.slots.contains_key(&24));
    let mut unknown = item(70001, 5, 0);
    unknown.flags = 0x40;
    let c = Catalog::new(vec![unknown], audit()).unwrap();
    assert!(c.grant_ready(70001).is_err());
    assert!(state().plan_destroy_backpack(&c, 24, 0).is_err());
}
#[test]
fn missing_selected_item_and_receipt_tampering_fail_closed() {
    let c = catalog(5, 0);
    let s = state();
    let missing = s.plan_destroy_backpack(&c, 38, 0).unwrap();
    assert_eq!(missing.result, InventoryResult::ItemNotFound);
    assert_eq!(missing.before, missing.after);
    for fault in 0..4 {
        let mut r = s.plan_destroy_backpack(&c, 24, 1).unwrap();
        match fault {
            0 => r.slot = 23,
            1 => r.removed = 2,
            2 => r.after.next_item += 1,
            _ => r.after.slots.get_mut(&23).unwrap().count -= 1,
        }
        assert!(r.validate(&c).is_err());
    }
    assert!(s.plan_destroy_backpack(&c, 22, 0).is_err());
}
