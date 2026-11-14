// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-shaped rows, not redistributed database content.
use super::*;

fn carried_item(shape: u32) -> Item {
    let mut row = item(70001, 5, 0);
    for spell in &mut row.spells {
        spell.cooldown = -1;
        spell.category_cooldown = -1;
    }
    match shape {
        0 => row.maximum_durability = 37,
        1 => {
            row.spells[0].id = 81001;
            row.spells[0].charges = -1;
            row.spells[0].category_cooldown = 1000;
        }
        2..=6 => {
            row.class = 1;
            row.inventory_type = 18;
            row.container_slots = 6;
            row.stackable = 1;
            row.spells[0].charges = -5;
            row.maximum_durability = 23;
        }
        _ => unreachable!(),
    }
    row
}

#[test]
fn corpse_carried_shapes_preserve_unused_sentinels_charges_durability_and_empty_bags() {
    for shape in 0..7 {
        let row = carried_item(shape);
        let catalog = Catalog::new(vec![row.clone()], audit()).unwrap();
        assert_eq!(catalog.corpse_carried_ready(70001).unwrap(), &row);
        assert!(catalog.ordinary_loot_ready(70001).is_err());
        assert!(catalog.equip_ready(70001).is_err());
        assert!(catalog.use_ready(70001).is_err());
        let mut state = State::new(BTreeMap::new(), 100).unwrap();
        assert!(state.can_store(&catalog, 70001, 1).is_err());
        assert!(state
            .store_new(&catalog, 70001, 1, |_, _, _| panic!())
            .is_err());
        let stored = state
            .store_corpse_item(&catalog, 70001, 1, |pre, entry, count| {
                assert_eq!((entry, count, pre.next_item), (70001, 1, 101));
                assert!(pre.slots.is_empty());
                Ok(())
            })
            .unwrap();
        let expected_bag = shape >= 2;
        assert_eq!(
            stored.last.kind,
            if expected_bag {
                InstanceKind::EmptyBag { slots: 6 }
            } else {
                InstanceKind::Item
            }
        );
        assert_eq!(
            stored.last.charges,
            if expected_bag {
                [0; 5]
            } else {
                row.spells.map(|s| s.charges)
            }
        );
        assert_eq!(stored.last.durability, row.maximum_durability);
        assert_eq!(stored.last.maximum_durability, row.maximum_durability);
        assert_eq!(catalog.item(70001).unwrap(), &row);
        let restored: State = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(state, restored);
        restored.validate(&catalog).unwrap();
    }
}

#[test]
fn spell_id_zero_never_erases_raw_fields_or_creation_charges() {
    let mut row = carried_item(0);
    row.spells = std::array::from_fn(|i| Spell {
        id: 0,
        trigger: i as u32,
        charges: i as i32 - 2,
        ppm_rate_bits: (i as f32 * 0.25).to_bits(),
        cooldown: -1,
        category: i as u32 + 10,
        category_cooldown: 1000 + i as i32,
    });
    let catalog = Catalog::new(vec![row.clone()], audit()).unwrap();
    let mut state = State::new(BTreeMap::new(), 1).unwrap();
    state
        .store_corpse_item(&catalog, 70001, 1, |_, _, _| Ok(()))
        .unwrap();
    assert_eq!(state.slots[&23].charges, [-2, -1, 0, 1, 2]);
    for field in 0..7 {
        let mut changed = row.clone();
        let spell = &mut changed.spells[0];
        match field {
            0 => spell.id = 1,
            1 => spell.trigger += 1,
            2 => spell.charges += 1,
            3 => spell.ppm_rate_bits = 1.0f32.to_bits(),
            4 => spell.cooldown = 0,
            5 => spell.category += 1,
            _ => spell.category_cooldown += 1,
        }
        assert_ne!(
            catalog.hash(),
            Catalog::new(vec![changed], audit()).unwrap().hash()
        );
    }
}

#[test]
fn corpse_storage_is_all_or_nothing_at_capacity_unique_and_id_boundaries() {
    for unique in [false, true] {
        let mut row = carried_item(2);
        row.maximum_count = if unique { 1 } else { 0 };
        let catalog = Catalog::new(vec![row], audit()).unwrap();
        let mut state = State::new(BTreeMap::new(), 1).unwrap();
        let count = if unique { 1 } else { 15 };
        state
            .store_corpse_item(&catalog, 70001, count, |_, _, _| Ok(()))
            .unwrap();
        let before = state.clone();
        let plan = state.can_store_corpse_item(&catalog, 70001, 2).unwrap();
        assert_eq!(
            plan.result,
            if unique {
                InventoryResult::CantCarryMoreOfThis
            } else {
                InventoryResult::InventoryFull
            }
        );
        assert!(matches!(
            state.store_corpse_item(&catalog, 70001, 2, |_, _, _| panic!("failed plan callback")),
            Err(Error::Rejected(_))
        ));
        assert_eq!(state, before);
        assert_eq!(state.slots.len(), count as usize);
    }
    let catalog = Catalog::new(vec![carried_item(0)], audit()).unwrap();
    for next_item in [u64::MAX, u64::MAX - 1] {
        let mut state = State::new(BTreeMap::new(), next_item).unwrap();
        let before = state.clone();
        assert_eq!(
            state.store_corpse_item(&catalog, 70001, 6, |_, _, _| panic!("allocation callback")),
            Err(Error::Exhausted)
        );
        assert_eq!(state, before);
    }
    let mut state = State::new(BTreeMap::new(), 1).unwrap();
    let before = state.clone();
    assert_eq!(
        state.store_corpse_item(&catalog, 70001, 6, |pre, _, _| {
            assert_eq!(pre.next_item, 2);
            assert!(pre.slots.is_empty());
            Err(Error::Callback("fault"))
        }),
        Err(Error::Callback("fault"))
    );
    assert_eq!(state, before);
}

#[test]
fn corpse_storage_binds_new_and_merged_instances_without_admin_self_unbinding() {
    for bonding in 0..=4 {
        let mut row = carried_item(1);
        row.bonding = bonding;
        let catalog = Catalog::new(vec![row], audit()).unwrap();
        let mut state = State::new(BTreeMap::new(), 1).unwrap();
        state
            .grant_item(
                &catalog,
                70001,
                1,
                GrantTarget::SelfPlayer,
                |_, _, _| Ok(()),
            )
            .unwrap();
        assert!(!state.slots[&23].bound);
        let id = state.slots[&23].id;
        let stored = state
            .store_corpse_item(&catalog, 70001, 5, |pre, _, count| {
                assert_eq!(count, 5);
                assert_eq!(pre.item_count(70001).unwrap(), 1);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            stored.destinations,
            vec![
                Destination { slot: 23, count: 4 },
                Destination { slot: 24, count: 1 }
            ]
        );
        assert_eq!(state.slots[&23].id, id);
        for instance in state.slots.values() {
            assert_eq!(instance.bound, matches!(bonding, 1 | 4));
            assert_eq!(instance.charges[0], -1);
        }
    }
}

#[test]
fn corpse_readiness_rejects_unsupported_dependencies_without_mutation_or_erasure() {
    for field in 0..18 {
        let mut row = carried_item(0);
        match field {
            0 => row.class = 11,
            1 => row.inventory_type = 27,
            2 => row.bag_family = 9,
            3 => row.start_quest = 1,
            4 => row.item_set = 1,
            5 => row.lock_id = 1,
            6 => row.random_property = 1,
            7 => row.duration = 1,
            8 => row.area = 1,
            9 => row.map = 1,
            10 => row.script_id = 1,
            11 => row.flags = 1,
            12 => row.extra_flags = 1,
            13 => row.class = 1,
            14 => row.inventory_type = 18,
            15 => row.container_slots = 6,
            16 => row.bag_family = 1,
            _ => {
                row.spells[0].id = 81001;
                row.spells[0].trigger = 1;
            }
        }
        let catalog = Catalog::new(vec![row.clone()], audit()).unwrap();
        let mut state = State::new(BTreeMap::new(), 1).unwrap();
        let before = state.clone();
        assert!(
            state
                .store_corpse_item(&catalog, 70001, 1, |_, _, _| panic!())
                .is_err(),
            "field {field}"
        );
        assert_eq!(state, before);
        assert_eq!(catalog.item(70001).unwrap(), &row);
    }
    for field in 0..5 {
        for evidence in [Evidence::Present, Evidence::Unknown] {
            let mut a = audit();
            match field {
                0 => a.item_creation_and_loot_scripts = evidence,
                1 => a.equipped_bags = evidence,
                2 => a.bank_and_keyring_items = evidence,
                3 => a.trade_or_temporary_item_loot = evidence,
                _ => a.additional_item_mutations = evidence,
            }
            let catalog = Catalog::new(vec![carried_item(0)], a).unwrap();
            assert!(catalog.corpse_carried_ready(70001).is_err());
        }
    }
}

#[test]
fn independent_source_corpse_storage_never_uses_admin_partial_or_unbinding_paths() {
    let mut cases = 0;
    for line in include_str!("../fixtures/inventory_corpse.tsv").lines() {
        let c: Vec<_> = line.split('\t').collect();
        let v: Vec<i64> = c[2].split(':').map(|s| s.parse().unwrap()).collect();
        let mut row = item(70001, v[0] as i32, v[1] as i32);
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
        let catalog = Catalog::new(vec![row, item(70002, 1, 0)], audit()).unwrap();
        let equipped = c[3].parse::<u32>().unwrap();
        let mut state = State::new(
            if equipped == 0 {
                BTreeMap::new()
            } else {
                BTreeMap::from([(70001, equipped)])
            },
            1000,
        )
        .unwrap();
        state.slots = grants::grant_slots(c[4]);
        let before = state.clone();
        let requested = c[1].parse().unwrap();
        let plan = state
            .can_store_corpse_item(&catalog, 70001, requested)
            .unwrap();
        assert_eq!(
            plan.result.protocol(),
            c[5].parse::<u8>().unwrap(),
            "case {}",
            c[0]
        );
        assert_eq!(plan.no_space_count, c[6].parse::<u32>().unwrap());
        assert_eq!(plan.destinations, destinations(c[9]));
        let mut callback = [0u64; 4];
        let result = state.store_corpse_item(&catalog, 70001, requested, |pre, item, count| {
            callback = [
                u64::from(item),
                u64::from(count),
                u64::from(pre.item_count(item)?),
                pre.next_item,
            ];
            assert_eq!(pre.slots, before.slots);
            Ok(())
        });
        if plan.result == InventoryResult::Ok {
            let stored = result.unwrap();
            let last: Vec<u64> = c[10].split(':').map(|s| s.parse().unwrap()).collect();
            assert_eq!(
                (
                    u64::from(stored.last_slot),
                    stored.last.id,
                    u64::from(stored.last_count)
                ),
                (last[0], last[1], last[2])
            );
            assert_eq!(stored.before_slots, before.slots);
            assert_eq!(stored.after_slots, state.slots);
        } else {
            assert_eq!(result, Err(Error::Rejected(plan)));
            assert_eq!(state, before);
            assert_eq!(c[10], "-");
        }
        assert_eq!(state.next_item, c[7].parse::<u64>().unwrap());
        assert_eq!(
            callback.to_vec(),
            c[8].split(':')
                .map(|s| s.parse::<u64>().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(state.slots, grants::grant_slots(c[11]), "case {}", c[0]);
        state.validate(&catalog).unwrap();
        cases += 1;
    }
    assert_eq!(cases, 320);
}
