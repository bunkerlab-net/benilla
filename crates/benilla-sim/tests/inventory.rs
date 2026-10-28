// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::inventory::*;
use std::collections::BTreeMap;

fn audit() -> Audit {
    Audit {
        item_creation_and_loot_scripts: Evidence::Absent,
        equipped_bags: Evidence::Absent,
        bank_and_keyring_items: Evidence::Absent,
        trade_or_temporary_item_loot: Evidence::Absent,
        additional_item_mutations: Evidence::Absent,
    }
}
fn item(entry: u32, stackable: i32, maximum_count: i32) -> Item {
    Item {
        entry,
        class: 15,
        inventory_type: 0,
        display_id: 90001,
        quality: 1,
        flags: 0,
        extra_flags: 0,
        stackable,
        maximum_count,
        bonding: 0,
        maximum_durability: 0,
        bag_family: 0,
        container_slots: 0,
        random_property: 0,
        start_quest: 0,
        duration: 0,
        item_set: 0,
        area: 0,
        map: 0,
        lock_id: 0,
        script_id: 0,
        spells: [Spell {
            id: 0,
            trigger: 0,
            charges: 0,
            ppm_rate_bits: 0,
            cooldown: 0,
            category: 0,
            category_cooldown: 0,
        }; 5],
    }
}
fn catalog(stack: i32, maximum: i32) -> Catalog {
    Catalog::new(
        vec![item(70001, stack, maximum), item(70002, 1, 0)],
        audit(),
    )
    .unwrap()
}
fn instance(slot: u8, item: u32, count: u32) -> Instance {
    Instance {
        id: u64::from(slot) + 1,
        item,
        count,
        bound: false,
        charges: [0; 5],
        durability: 0,
        maximum_durability: 0,
        kind: InstanceKind::Item,
        duration: 0,
        random_property_id: 0,
        enchantments: [0; 3],
    }
}
fn recipe(case: u32) -> (Catalog, State, u32) {
    if case >= 2048 {
        let mut row = item(70001, 255, 0);
        row.bonding = (case - 2048) % 5;
        row.maximum_durability = (case - 2048) * 7;
        return (
            Catalog::new(vec![row], audit()).unwrap(),
            State::new(BTreeMap::new(), 1000).unwrap(),
            1 + (case - 2048) * 100,
        );
    }
    let stack = if case < 1024 {
        1 + case % 7
    } else {
        [1, 2, 7, 255][case as usize % 4]
    };
    let maximum = if case.is_multiple_of(4) {
        0
    } else {
        10 + case % 37
    };
    let mut state = State::new(
        if case.is_multiple_of(5) {
            BTreeMap::from([(70001, 1)])
        } else {
            BTreeMap::new()
        },
        1000,
    )
    .unwrap();
    for slot in BACKPACK_START..BACKPACK_END {
        let mode = (case * 13 + u32::from(slot) * 7) % 5;
        if mode < 2 {
            state.slots.insert(
                slot,
                instance(slot, 70001, 1 + (case + u32::from(slot)) % stack),
            );
        } else if mode < 4 {
            state.slots.insert(slot, instance(slot, 70002, 1));
        }
    }
    let count = if case < 1024 {
        1 + case % 29
    } else {
        [1, 254, 255, 256, 257, 4096, 10000][case as usize % 7]
    };
    (catalog(stack as i32, maximum as i32), state, count)
}
fn destinations(text: &str) -> Vec<Destination> {
    if text == "-" {
        return Vec::new();
    }
    text.split(',')
        .map(|position| {
            let (slot, count) = position.split_once(':').unwrap();
            Destination {
                slot: slot.parse().unwrap(),
                count: count.parse().unwrap(),
            }
        })
        .collect()
}
fn stored_slots(text: &str) -> BTreeMap<u8, Instance> {
    text.split(',')
        .map(|instance| {
            let v: Vec<u64> = instance
                .split(':')
                .map(|part| part.parse().unwrap())
                .collect();
            (
                v[0] as u8,
                Instance {
                    id: v[1],
                    item: v[2] as u32,
                    count: v[3] as u32,
                    bound: v[4] == 1,
                    durability: v[5] as u32,
                    maximum_durability: v[6] as u32,
                    kind: InstanceKind::Item,
                    duration: 0,
                    random_property_id: 0,
                    enchantments: [0; 3],
                    charges: std::array::from_fn(|index| v[7 + index] as i32),
                },
            )
        })
        .collect()
}

#[test]
fn exact_pinned_source_storage_plans_and_shared_whole_positions() {
    let mut cases = 0;
    for line in include_str!("fixtures/inventory.tsv").lines() {
        let columns: Vec<&str> = line.split('\t').collect();
        let case: u32 = columns[0].parse().unwrap();
        let (catalog, state, count) = recipe(case);
        let before = state.clone();
        let mut shared = Vec::new();
        if (1024..2048).contains(&case) {
            for slot in BACKPACK_START..BACKPACK_END {
                if (case + u32::from(slot)).is_multiple_of(7) {
                    shared.push(Destination { slot, count: 1 });
                }
            }
        }
        let plan = state
            .can_store_with_destinations(&catalog, 70001, count, &mut shared)
            .unwrap();
        assert_eq!(
            plan.result.protocol(),
            columns[1].parse::<u8>().unwrap(),
            "case {case}"
        );
        assert_eq!(
            plan.no_space_count,
            columns[2].parse::<u32>().unwrap(),
            "case {case}"
        );
        assert_eq!(plan.destinations, destinations(columns[3]), "case {case}");
        assert_eq!(shared, plan.destinations);
        assert_eq!(state, before);
        cases += 1;
    }
    assert_eq!(cases, 1577);
}

#[test]
fn exact_pinned_source_creation_cloning_merging_metadata_and_callback() {
    let mut cases = 0;
    for line in include_str!("fixtures/inventory_store.tsv").lines() {
        let columns: Vec<&str> = line.split('\t').collect();
        let case: u32 = columns[0].parse().unwrap();
        let (catalog, mut state, count) = recipe(case);
        let before = state.clone();
        let callback: Vec<u32> = columns[2]
            .split(':')
            .map(|part| part.parse().unwrap())
            .collect();
        let mut called = false;
        let stored = state
            .store_new(&catalog, 70001, count, |pre, item, amount| {
                called = true;
                assert_eq!(
                    [
                        item,
                        amount,
                        pre.item_count(item).unwrap(),
                        pre.next_item as u32
                    ],
                    callback[..],
                    "case {case}"
                );
                assert_eq!(pre.slots, before.slots);
                Ok(())
            })
            .unwrap();
        assert!(called);
        assert_eq!(
            state.next_item,
            columns[1].parse::<u64>().unwrap(),
            "case {case}"
        );
        assert_eq!(state.slots, stored_slots(columns[4]), "case {case}");
        let last: Vec<u64> = columns[3]
            .split(':')
            .map(|part| part.parse().unwrap())
            .collect();
        assert_eq!(
            (stored.last_slot, stored.last.id, stored.last_count),
            (last[0] as u8, last[1], last[2] as u32),
            "case {case}"
        );
        assert_eq!(stored.before_slots, before.slots);
        assert_eq!(stored.after_slots, state.slots);
        assert_eq!(stored.last, state.slots[&stored.last_slot]);
        assert_eq!((stored.item, stored.count), (70001, count));
        assert_eq!(state.equipped, before.equipped);
        cases += 1;
    }
    assert_eq!(cases, 515);
}

#[test]
fn normalized_uint8_stack_boundary_and_original_id_order() {
    assert!(Catalog::new(vec![item(70001, 256, 0)], audit()).is_err());
    assert!(Catalog::new(vec![item(70001, 0, 0)], audit()).is_err());
    let catalog = catalog(255, 0);
    let mut state = State::new(BTreeMap::new(), 41).unwrap();
    let stored = state
        .store_new(&catalog, 70001, 256, |_, _, _| Ok(()))
        .unwrap();
    assert_eq!(
        stored.destinations,
        vec![
            Destination {
                slot: 23,
                count: 255
            },
            Destination { slot: 24, count: 1 }
        ]
    );
    assert_eq!(
        (state.slots[&23].id, state.slots[&24].id, state.next_item),
        (42, 41, 43)
    );
    assert_eq!((stored.last_count, stored.count), (1, 256));
    state
        .store_new(&catalog, 70001, 2, |_, _, _| Ok(()))
        .unwrap();
    assert_eq!(
        (state.slots[&24].id, state.slots[&24].count, state.next_item),
        (41, 3, 44)
    );
}

#[test]
fn unique_and_capacity_errors_retain_partial_plans_but_never_store() {
    let catalog = catalog(5, 3);
    let mut state = State::new(BTreeMap::from([(70001, 1)]), 1000).unwrap();
    let plan = state.can_store(&catalog, 70001, 5).unwrap();
    assert_eq!(
        plan,
        Plan {
            result: InventoryResult::CantCarryMoreOfThis,
            destinations: vec![Destination { slot: 23, count: 2 }],
            no_space_count: 3
        }
    );
    let before = state.clone();
    assert_eq!(
        state.store_new(&catalog, 70001, 5, |_, _, _| panic!("refused callback")),
        Err(Error::Rejected(plan))
    );
    assert_eq!(state, before);
    for slot in 23..38 {
        state.slots.insert(slot, instance(slot, 70002, 1));
    }
    let catalog = self::catalog(1, 0);
    let plan = state.can_store(&catalog, 70001, 3).unwrap();
    assert_eq!(
        plan,
        Plan {
            result: InventoryResult::InventoryFull,
            destinations: vec![Destination { slot: 38, count: 1 }],
            no_space_count: 2
        }
    );
    let before = state.clone();
    assert_eq!(
        state.store_new(&catalog, 70001, 3, |_, _, _| panic!("refused callback")),
        Err(Error::Rejected(plan))
    );
    assert_eq!(state, before);
}

#[test]
fn shared_reward_preflights_reserve_whole_positions_and_do_not_reserve_unique_counts() {
    let catalog = catalog(5, 1);
    let state = State::new(BTreeMap::new(), 1).unwrap();
    let mut shared = Vec::new();
    assert_eq!(
        state
            .can_store_with_destinations(&catalog, 70001, 1, &mut shared)
            .unwrap()
            .result,
        InventoryResult::Ok
    );
    assert_eq!(
        state
            .can_store_with_destinations(&catalog, 70001, 1, &mut shared)
            .unwrap()
            .result,
        InventoryResult::Ok
    );
    assert_eq!(
        shared,
        vec![
            Destination { slot: 23, count: 1 },
            Destination { slot: 24, count: 1 }
        ]
    );
    let mut state = state;
    state
        .store_new(&catalog, 70001, 1, |_, _, _| Ok(()))
        .unwrap();
    assert_eq!(
        state.can_store(&catalog, 70001, 1).unwrap().result,
        InventoryResult::CantCarryMoreOfThis
    );
    let catalog = self::catalog(5, 0);
    let mut shared = Vec::new();
    state
        .can_store_with_destinations(&catalog, 70001, 1, &mut shared)
        .unwrap();
    state
        .can_store_with_destinations(&catalog, 70001, 1, &mut shared)
        .unwrap();
    assert_eq!(
        shared,
        vec![
            Destination { slot: 23, count: 1 },
            Destination { slot: 24, count: 1 }
        ]
    );
}

#[test]
fn allocator_and_callback_failure_leave_all_inventory_unchanged() {
    let catalog = catalog(1, 0);
    for next_item in [u64::MAX, u64::MAX - 1] {
        let mut state = State::new(BTreeMap::new(), next_item).unwrap();
        let before = state.clone();
        assert_eq!(
            state.store_new(&catalog, 70001, 2, |_, _, _| panic!("exhausted callback")),
            Err(Error::Exhausted)
        );
        assert_eq!(state, before);
    }
    let mut state = State::new(BTreeMap::new(), 99).unwrap();
    let before = state.clone();
    assert_eq!(
        state.store_new(&catalog, 70001, 1, |_, _, _| Err(Error::Callback("test"))),
        Err(Error::Callback("test"))
    );
    assert_eq!(state, before);
    // A complete merge still creates and consumes the original item identity.
    let catalog = self::catalog(5, 0);
    state.slots.insert(23, instance(23, 70001, 1));
    state.next_item = u64::MAX;
    let before = state.clone();
    assert_eq!(
        state.store_new(&catalog, 70001, 1, |_, _, _| panic!("exhausted callback")),
        Err(Error::Exhausted)
    );
    assert_eq!(state, before);
}

#[test]
fn malformed_equipment_counts_instances_reservations_and_restored_catalog_reject() {
    for equipped in [
        BTreeMap::from([(0, 1)]),
        BTreeMap::from([(70001, 0)]),
        BTreeMap::from([(70001, u32::MAX)]),
        BTreeMap::from([(70001, 19), (70002, 1)]),
    ] {
        assert!(State::new(equipped, 1).is_err());
    }
    assert!(State::new(BTreeMap::new(), 0).is_err());
    let catalog = catalog(5, 0);
    let mut state = State::new(BTreeMap::new(), 1000).unwrap();
    state.equipped.insert(70001, u32::MAX);
    assert!(state.validate(&catalog).is_err());
    assert!(state.item_count(70001).is_err());
    state.equipped.clear();
    for bad in [0, 6, u32::MAX] {
        state.slots.insert(23, instance(23, 70001, bad));
        assert!(state.validate(&catalog).is_err());
    }
    state.slots.clear();
    state.slots.insert(23, instance(23, 70001, 1));
    state.slots.insert(24, instance(23, 70001, 1));
    assert!(state.validate(&catalog).is_err());
    state.slots.clear();
    for mut reserved in [
        vec![Destination { slot: 22, count: 1 }],
        vec![Destination {
            slot: 23,
            count: 256,
        }],
        vec![Destination { slot: 23, count: 1 }; 2],
    ] {
        assert!(state
            .can_store_with_destinations(&catalog, 70001, 1, &mut reserved)
            .is_err());
    }
    for count in [0, 10001, u32::MAX] {
        assert!(state.can_store(&catalog, 70001, count).is_err());
    }
    assert_eq!(
        state.can_store(&catalog, 99, 1).unwrap(),
        Plan {
            result: InventoryResult::ItemNotFound,
            destinations: vec![],
            no_space_count: 1
        }
    );
    let mut encoded = serde_json::to_value(&catalog).unwrap();
    encoded["rows"]["70001"]["stackable"] = 256.into();
    let corrupted: Catalog = serde_json::from_value(encoded).unwrap();
    assert!(corrupted.validate().is_err());
    assert!(state.can_store(&corrupted, 70001, 1).is_err());
}

#[test]
fn unsupported_template_dependencies_and_incomplete_audits_reject() {
    for evidence in [Evidence::Unknown, Evidence::Present] {
        for field in 0..5 {
            let mut audit = audit();
            match field {
                0 => audit.item_creation_and_loot_scripts = evidence,
                1 => audit.equipped_bags = evidence,
                2 => audit.bank_and_keyring_items = evidence,
                3 => audit.trade_or_temporary_item_loot = evidence,
                _ => audit.additional_item_mutations = evidence,
            }
            let catalog = Catalog::new(vec![item(70001, 5, 0)], audit).unwrap();
            assert!(catalog.ordinary_loot_ready(70001).is_err());
            assert!(catalog.grant_ready(70001).is_err());
        }
    }
    for field in 0..19 {
        let mut row = item(70001, 5, 0);
        match field {
            0 => row.class = 1,
            1 => row.class = 11,
            2 => row.inventory_type = 18,
            3 => row.inventory_type = 27,
            4 => row.flags = 1,
            5 => row.extra_flags = 1,
            6 => row.bag_family = 256,
            7 => row.container_slots = 1,
            8 => row.random_property = 1,
            9 => row.start_quest = 1,
            10 => row.duration = 1,
            11 => row.item_set = 1,
            12 => row.area = 1,
            13 => row.map = 1,
            14 => row.lock_id = 1,
            15 => row.spells[0].id = 1,
            16 => row.spells[0].charges = 1,
            17 => row.spells[0].trigger = 1,
            _ => row.maximum_count = -1,
        }
        let catalog = Catalog::new(vec![row], audit());
        if field == 18 {
            assert!(catalog.is_err());
        } else {
            assert!(
                catalog.unwrap().ordinary_loot_ready(70001).is_err(),
                "field {field}"
            );
        }
    }
    let mut row = item(70001, 5, 0);
    row.flags = 0x800;
    assert!(Catalog::new(vec![row.clone()], audit()).is_ok());
    assert!(Catalog::new(vec![row.clone(), row], audit()).is_err());
}

#[test]
fn source_order_removal_preserves_equipment_and_does_not_allocate() {
    let catalog = catalog(5, 0);
    let mut state = State::new(BTreeMap::from([(70001, 1)]), 100).unwrap();
    state.slots.insert(23, instance(23, 70001, 2));
    state.slots.insert(24, instance(24, 70002, 1));
    state.slots.insert(26, instance(26, 70001, 5));
    assert_eq!(state.removal_steps(&catalog, 70001, 6).unwrap(), vec![2, 4]);
    assert!(state.removal_steps(&catalog, 70001, 8).is_err());
    let before = state.clone();
    assert!(state.remove_first_stack(&catalog, 70001, 3).is_err());
    assert_eq!(state, before);
    state.remove_first_stack(&catalog, 70001, 2).unwrap();
    state.remove_first_stack(&catalog, 70001, 4).unwrap();
    assert_eq!(state.slots[&26].count, 1);
    assert_eq!(state.item_count(70001).unwrap(), 2);
    assert_eq!(state.equipped, before.equipped);
    assert_eq!(state.next_item, before.next_item);
}

#[test]
fn existing_unique_overage_and_metadata_tampering_reject_before_arithmetic() {
    let mut state = State::new(BTreeMap::from([(70001, 2)]), 100).unwrap();
    let catalog = catalog(5, 1);
    assert_eq!(
        state.can_store(&catalog, 70001, 1),
        Err(Error::Invalid("existing_unique_count"))
    );
    state.equipped.clear();
    state.slots.insert(23, instance(23, 70001, 2));
    assert_eq!(
        state.can_store(&catalog, 70001, 1),
        Err(Error::Invalid("existing_unique_count"))
    );
    let catalog = self::catalog(5, 0);
    for field in 0..6 {
        let mut bad = state.clone();
        let item = bad.slots.get_mut(&23).unwrap();
        match field {
            0 => item.bound = true,
            1 => item.charges[3] = 1,
            2 => item.durability = 1,
            3 => item.maximum_durability = 1,
            4 => item.id = 0,
            _ => item.id = 100,
        }
        assert!(bad.can_store(&catalog, 70001, 1).is_err(), "field {field}");
    }
    state.slots.insert(22, instance(22, 70001, 1));
    assert!(state.validate(&catalog).is_err());
}

#[test]
fn catalog_hash_is_canonical_across_row_order_and_serialized_roundtrip() {
    let first = item(70001, 5, 0);
    let second = item(70002, 1, 0);
    let a = Catalog::new(vec![first.clone(), second.clone()], audit()).unwrap();
    let b = Catalog::new(vec![second, first.clone()], audit()).unwrap();
    assert_eq!(a.hash(), b.hash());
    let restored: Catalog = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(a.hash(), restored.hash());
    let mut changed = first;
    changed.display_id += 1;
    assert_ne!(
        a.hash(),
        Catalog::new(vec![changed, item(70002, 1, 0)], audit())
            .unwrap()
            .hash()
    );
}

#[path = "inventory/grants.rs"]
mod grants;

#[path = "inventory/movement.rs"]
mod movement;

#[path = "inventory/corpse.rs"]
mod corpse;

#[path = "inventory/split.rs"]
mod split;

#[path = "inventory/destroy.rs"]
mod destroy;
