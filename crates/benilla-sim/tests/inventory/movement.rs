// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn movement_slots(text: &str) -> BTreeMap<u8, Instance> {
    if text == "-" {
        return BTreeMap::new();
    }
    text.split(',')
        .map(|row| {
            let v: Vec<i64> = row.split(':').map(|v| v.parse().unwrap()).collect();
            assert_eq!(v.len(), 15);
            (
                v[0] as u8,
                Instance {
                    id: v[1] as u64,
                    item: v[2] as u32,
                    count: v[3] as u32,
                    bound: v[4] == 1,
                    durability: v[5] as u32,
                    maximum_durability: v[6] as u32,
                    charges: std::array::from_fn(|i| v[7 + i] as i32),
                    kind: if v[12] == 1 {
                        InstanceKind::EmptyBag {
                            slots: v[13] as u32,
                        }
                    } else {
                        InstanceKind::Item
                    },
                    duration: v[14] as u32,
                    random_property_id: 0,
                    enchantments: [0; 3],
                },
            )
        })
        .collect()
}

fn movement_catalog(text: &str) -> Catalog {
    let rows = text
        .split(',')
        .map(|row| {
            let values: Vec<i64> = row.split(':').map(|v| v.parse().unwrap()).collect();
            assert_eq!(values.len(), 46);
            let v = &values[1..];
            let mut row = item(values[0] as u32, v[0] as i32, v[1] as i32);
            row.class = v[2] as u32;
            row.inventory_type = v[3] as u32;
            row.container_slots = v[4] as u32;
            row.bonding = v[5] as u32;
            row.maximum_durability = v[6] as u32;
            row.bag_family = v[7] as u32;
            row.start_quest = v[8] as u32;
            row.item_set = v[9] as u32;
            row.spells = std::array::from_fn(|i| Spell {
                id: v[10 + i * 7] as u32,
                trigger: v[11 + i * 7] as u32,
                charges: v[12 + i * 7] as i32,
                ppm_rate_bits: v[13 + i * 7] as u32,
                cooldown: v[14 + i * 7] as i32,
                category: v[15 + i * 7] as u32,
                category_cooldown: v[16 + i * 7] as i32,
            });
            row
        })
        .collect();
    Catalog::new(rows, audit()).unwrap()
}

#[test]
fn independent_pinned_swapitem_matches_complete_states_and_source_errors() {
    let mut cases = 0;
    let mut outcomes = [0; 7];
    for line in include_str!("../fixtures/inventory_movement.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 12);
        let context = format!("case {}", columns[0]);
        let catalog = movement_catalog(columns[4]);
        let mut state = State::new(BTreeMap::new(), 1000).unwrap();
        state.slots = movement_slots(columns[5]);
        let before = state.clone();
        let receipt = state
            .swap_backpack(
                &catalog,
                columns[2].parse().unwrap(),
                columns[3].parse().unwrap(),
                columns[1] == "1",
            )
            .unwrap();
        assert_eq!(
            receipt.result.protocol(),
            columns[6].parse::<u8>().unwrap(),
            "{context}"
        );
        assert_eq!(columns[7], "0", "{context}");
        assert_eq!(
            state.next_item,
            columns[8].parse::<u64>().unwrap(),
            "{context}"
        );
        assert_eq!(state.slots, movement_slots(columns[9]), "{context}");
        for (slot, column) in [(receipt.source, 10), (receipt.destination, 11)] {
            assert_eq!(
                if receipt.result == InventoryResult::Ok {
                    0
                } else {
                    before.slots.get(&slot).map_or(0, |i| i.id)
                },
                columns[column].parse::<u64>().unwrap(),
                "{context} error GUID"
            );
        }
        assert_eq!(receipt.before, before, "{context}");
        assert_eq!(receipt.after, state, "{context}");
        assert_eq!(state.equipped, before.equipped, "{context}");
        assert_eq!(state.next_item, before.next_item, "{context}");
        for row in catalog.items() {
            assert_eq!(
                state.item_count(row.entry),
                before.item_count(row.entry),
                "{context}"
            );
        }
        outcomes[match receipt.outcome {
            MovementOutcome::IgnoredSameSlot => 0,
            MovementOutcome::IgnoredEmptySource => 1,
            MovementOutcome::Rejected => 2,
            MovementOutcome::Moved => 3,
            MovementOutcome::Swapped => 4,
            MovementOutcome::Merged {
                removed_source: true,
                ..
            } => 5,
            MovementOutcome::Merged {
                removed_source: false,
                ..
            } => 6,
        }] += 1;
        receipt.validate(&catalog).unwrap();
        cases += 1;
    }
    assert_eq!(cases, 2880);
    assert!(outcomes.iter().all(|&count| count > 0));
}

fn seeded(catalog: &Catalog, source_count: u32, destination_count: u32) -> State {
    let mut state = State::new(BTreeMap::new(), 1000).unwrap();
    state.slots.insert(23, instance(23, 70001, source_count));
    if destination_count != 0 {
        state
            .slots
            .insert(38, instance(38, 70001, destination_count));
    }
    state.validate(catalog).unwrap();
    state
}

#[test]
fn all_slot_pairs_preserve_instances_allocator_and_immutable_equipment() {
    let catalog = catalog(5, 0);
    for source in BACKPACK_START..BACKPACK_END {
        for destination in BACKPACK_START..BACKPACK_END {
            let mut state = State::new(BTreeMap::from([(70002, 1)]), u64::MAX).unwrap();
            let original = instance(source, 70001, 3);
            state.slots.insert(source, original.clone());
            let receipt = state
                .swap_backpack(&catalog, source, destination, true)
                .unwrap();
            assert_eq!(state.slots, BTreeMap::from([(destination, original)]));
            assert_eq!(state.next_item, u64::MAX);
            assert_eq!(state.equipped, receipt.before.equipped);
            assert_eq!(receipt.after, state);
            assert_eq!(receipt.result, InventoryResult::Ok);
            assert_eq!(
                receipt.outcome,
                if source == destination {
                    MovementOutcome::IgnoredSameSlot
                } else {
                    MovementOutcome::Moved
                }
            );
            receipt.validate(&catalog).unwrap();
        }
    }
}

#[test]
fn packet_same_slot_and_empty_source_precede_dead_error() {
    let catalog = catalog(5, 0);
    let initial = seeded(&catalog, 3, 2);
    for (source, destination, expected) in [
        (23, 23, MovementOutcome::IgnoredSameSlot),
        (24, 24, MovementOutcome::IgnoredSameSlot),
        (24, 23, MovementOutcome::IgnoredEmptySource),
        (24, 25, MovementOutcome::IgnoredEmptySource),
        (23, 38, MovementOutcome::Rejected),
        (23, 24, MovementOutcome::Rejected),
    ] {
        let mut state = initial.clone();
        let receipt = state
            .swap_backpack(&catalog, source, destination, false)
            .unwrap();
        assert_eq!(receipt.outcome, expected);
        assert_eq!(state, initial);
        assert_eq!(receipt.before, receipt.after);
        assert_eq!(
            receipt.result.protocol(),
            if expected == MovementOutcome::Rejected {
                38
            } else {
                0
            }
        );
        receipt.validate(&catalog).unwrap();
    }
}

#[test]
fn complete_merge_consumes_only_source_identity_partial_merge_retains_both() {
    let catalog = catalog(5, 0);
    for (source, destination, moved, remains) in [(2, 3, 2, 0), (1, 2, 1, 0), (5, 3, 2, 3)] {
        let mut state = seeded(&catalog, source, destination);
        let before = state.clone();
        let receipt = state.swap_backpack(&catalog, 23, 38, true).unwrap();
        assert_eq!(
            receipt.outcome,
            MovementOutcome::Merged {
                transferred: moved,
                removed_source: remains == 0,
            }
        );
        assert_eq!(state.slots[&38].id, before.slots[&38].id);
        assert_eq!(state.slots[&38].count, destination + moved);
        if remains == 0 {
            assert!(!state.slots.contains_key(&23));
        } else {
            assert_eq!(state.slots[&23].id, before.slots[&23].id);
            assert_eq!(state.slots[&23].count, remains);
        }
        assert_eq!(state.next_item, before.next_item);
        assert_eq!(state.item_count(70001), before.item_count(70001));
        receipt.validate(&catalog).unwrap();
    }
}

#[test]
fn full_target_stack_swaps_and_partial_fill_uses_no_other_slots_even_when_full() {
    let catalog = catalog(255, 0);
    let mut state = seeded(&catalog, 255, 255);
    let before = state.clone();
    let receipt = state.swap_backpack(&catalog, 23, 38, true).unwrap();
    assert_eq!(receipt.outcome, MovementOutcome::Swapped);
    assert_eq!(state.slots[&23], before.slots[&38]);
    assert_eq!(state.slots[&38], before.slots[&23]);
    state.slots.get_mut(&38).unwrap().count = 254;
    for slot in 24..38 {
        state.slots.insert(slot, instance(slot, 70002, 1));
    }
    let before = state.clone();
    let receipt = state.swap_backpack(&catalog, 23, 38, true).unwrap();
    assert_eq!(
        receipt.outcome,
        MovementOutcome::Merged {
            transferred: 1,
            removed_source: false,
        }
    );
    assert_eq!(state.slots[&23].count, 254);
    assert_eq!(state.slots[&38].count, 255);
    for slot in 24..38 {
        assert_eq!(state.slots[&slot], before.slots[&slot]);
    }
}

#[test]
fn swap_different_items_preserves_metadata_and_unique_counts_exclude_moving_source() {
    let mut first = item(70001, 5, 4);
    first.spells[0].charges = -7;
    first.spells[0].id = 90001;
    first.maximum_durability = 17;
    let catalog = Catalog::new(vec![first, item(70002, 1, 1)], audit()).unwrap();
    let mut state = State::new(BTreeMap::from([(70001, 1)]), 1000).unwrap();
    let mut source = instance(23, 70001, 3);
    source.charges[0] = -7;
    source.durability = 17;
    source.maximum_durability = 17;
    state.slots.insert(23, source);
    state.slots.insert(38, instance(38, 70002, 1));
    let before = state.clone();
    let receipt = state.swap_backpack(&catalog, 23, 38, true).unwrap();
    assert_eq!(receipt.outcome, MovementOutcome::Swapped);
    assert_eq!(state.slots[&23], before.slots[&38]);
    assert_eq!(state.slots[&38], before.slots[&23]);
    assert_eq!(state.equipped, before.equipped);
    receipt.validate(&catalog).unwrap();
}

#[test]
fn source_store_binding_differs_from_direct_partial_fill() {
    for bonding in 0..=4 {
        let mut row = item(70001, 5, 0);
        row.bonding = bonding;
        let catalog = Catalog::new(vec![row], audit()).unwrap();
        for (source_count, destination_count, binds) in
            [(2, 0, true), (2, 3, true), (2, 5, true), (5, 3, false)]
        {
            let mut state = seeded(&catalog, source_count, destination_count);
            let receipt = state.swap_backpack(&catalog, 23, 38, true).unwrap();
            for instance in state.slots.values() {
                assert_eq!(instance.bound, binds && matches!(bonding, 1 | 4));
            }
            receipt.validate(&catalog).unwrap();
        }
    }
}

#[test]
fn carried_empty_bags_swap_instead_of_merging_without_adding_usable_slots() {
    let mut row = item(70001, 5, 0);
    row.class = 1;
    row.inventory_type = 18;
    row.container_slots = 36;
    let catalog = Catalog::new(vec![row], audit()).unwrap();
    let mut state = State::new(BTreeMap::new(), 1000).unwrap();
    for slot in [23, 38] {
        let mut bag = instance(slot, 70001, 1);
        bag.kind = InstanceKind::EmptyBag { slots: 36 };
        state.slots.insert(slot, bag);
    }
    let before = state.clone();
    let receipt = state.swap_backpack(&catalog, 23, 38, true).unwrap();
    assert_eq!(receipt.outcome, MovementOutcome::Swapped);
    assert_eq!(state.slots[&23], before.slots[&38]);
    assert_eq!(state.slots[&38], before.slots[&23]);
    assert_eq!(state.slots.len(), 2);
    receipt.validate(&catalog).unwrap();
}

#[test]
fn unsupported_positions_audits_and_corrupt_state_refuse_atomically() {
    let catalog = catalog(5, 0);
    let mut state = seeded(&catalog, 2, 0);
    for invalid in [0, 18, 19, 22, 39, 63, 81, 255] {
        for (source, destination) in [(invalid, 23), (23, invalid), (invalid, invalid)] {
            let before = state.clone();
            assert_eq!(
                state.swap_backpack(&catalog, source, destination, true),
                Err(Error::Unsupported("backpack_movement_position"))
            );
            assert_eq!(state, before);
        }
    }
    for evidence in [Evidence::Present, Evidence::Unknown] {
        let mut excluded = audit();
        excluded.additional_item_mutations = evidence;
        let catalog = Catalog::new(vec![item(70001, 5, 0)], excluded).unwrap();
        for mut state in [state.clone(), State::new(BTreeMap::new(), 1).unwrap()] {
            let before = state.clone();
            assert!(matches!(
                state.swap_backpack(&catalog, 23, 38, true),
                Err(Error::Unsupported(_))
            ));
            assert_eq!(state, before);
        }
    }
    state.slots.get_mut(&23).unwrap().count = 256;
    let before = state.clone();
    assert!(state.swap_backpack(&catalog, 23, 38, true).is_err());
    assert_eq!(state, before);
}

#[test]
fn ambiguous_twohand_offhand_equipment_refuses_without_inventing_source_error() {
    for inventory_type in [13, 14, 22, 23] {
        let mut twohand = item(70003, 1, 0);
        twohand.inventory_type = 17;
        let mut offhand = item(70004, 1, 0);
        offhand.inventory_type = inventory_type;
        let catalog = Catalog::new(
            vec![item(70001, 5, 0), item(70002, 1, 0), twohand, offhand],
            audit(),
        )
        .unwrap();
        for equipped in [
            BTreeMap::from([(70003, 1), (70004, 1)]),
            BTreeMap::from([(70003, 2)]),
        ] {
            for (source, destination, alive) in [(23, 38, true), (23, 23, true), (23, 38, false)] {
                let mut state = seeded(&catalog, 2, 0);
                state.equipped = equipped.clone();
                let before = state.clone();
                assert_eq!(
                    state.swap_backpack(&catalog, source, destination, alive),
                    Err(Error::Unsupported("backpack_movement_offhand_dependency"))
                );
                assert_eq!(state, before);
            }
        }
        for equipped in [
            BTreeMap::from([(70003, 1)]),
            BTreeMap::from([(70004, 1)]),
            BTreeMap::new(),
        ] {
            let mut state = seeded(&catalog, 2, 0);
            state.slots.insert(38, instance(38, 70002, 1));
            state.equipped = equipped.clone();
            let receipt = state.swap_backpack(&catalog, 23, 38, true).unwrap();
            assert_eq!(receipt.outcome, MovementOutcome::Swapped);
            assert_eq!(state.equipped, equipped);
            receipt.validate(&catalog).unwrap();
        }
    }
}

#[test]
fn receipt_recomputes_and_rejects_tampered_metadata_results_and_transition() {
    let catalog = catalog(5, 0);
    let receipt = seeded(&catalog, 2, 3)
        .swap_backpack(&catalog, 23, 38, true)
        .unwrap();
    for field in 0..11 {
        let mut bad = receipt.clone();
        match field {
            0 => bad.after.slots.get_mut(&38).unwrap().id = 900,
            1 => bad.after.slots.get_mut(&38).unwrap().count = 4,
            2 => bad.after.slots.get_mut(&38).unwrap().bound = true,
            3 => bad.after.next_item += 1,
            4 => {
                bad.after.equipped.insert(70002, 1);
            }
            5 => bad.source = 24,
            6 => bad.destination = 37,
            7 => bad.alive = false,
            8 => bad.result = InventoryResult::YouAreDead,
            9 => bad.outcome = MovementOutcome::Moved,
            _ => bad.before.slots.get_mut(&23).unwrap().count += 1,
        }
        assert!(bad.validate(&catalog).is_err(), "field {field}");
    }
    let json = serde_json::to_string(&receipt).unwrap();
    let restored: MovementReceipt = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, receipt);
    restored.validate(&catalog).unwrap();
    let mut encoded = serde_json::to_value(&receipt).unwrap();
    encoded["after"]["slots"]["38"]["bag_contents"] = serde_json::json!([70001]);
    assert!(serde_json::from_value::<MovementReceipt>(encoded).is_err());
}
