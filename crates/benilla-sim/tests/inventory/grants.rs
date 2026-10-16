// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

pub(super) fn grant_slots(text: &str) -> BTreeMap<u8, Instance> {
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
#[test]
fn independent_additem_source_partial_self_other_bags_and_creation_metadata() {
    let mut cases = 0;
    for line in include_str!("../fixtures/inventory_grant.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        let v: Vec<i64> = columns[3].split(':').map(|v| v.parse().unwrap()).collect();
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
        let equipped: u32 = columns[4].parse().unwrap();
        let mut state = State::new(
            if equipped == 0 {
                BTreeMap::new()
            } else {
                BTreeMap::from([(70001, equipped)])
            },
            1000,
        )
        .unwrap();
        state.slots = grant_slots(columns[5]);
        let before = state.clone();
        let mut callback = [0; 4];
        let requested = columns[2].parse().unwrap();
        let target = if columns[1] == "0" {
            GrantTarget::SelfPlayer
        } else {
            GrantTarget::OtherPlayer
        };
        let receipt = state
            .grant_item(&catalog, 70001, requested, target, |pre, item, count| {
                callback = [
                    u64::from(item),
                    u64::from(count),
                    u64::from(pre.item_count(item)?),
                    pre.next_item,
                ];
                Ok(())
            })
            .unwrap();
        let context = format!("case {} target {}", columns[0], columns[1]);
        assert_eq!(receipt.granted > 0, columns[6] == "1", "{context}");
        assert_eq!(
            receipt.storage_result.protocol(),
            columns[7].parse::<u8>().unwrap(),
            "{context}"
        );
        assert_eq!(
            receipt.ungranted,
            columns[8].parse::<u32>().unwrap(),
            "{context}"
        );
        assert_eq!(
            receipt.granted,
            columns[9].parse::<u32>().unwrap(),
            "{context}"
        );
        assert_eq!(
            state.next_item,
            columns[10].parse::<u64>().unwrap(),
            "{context}"
        );
        assert_eq!(
            callback.to_vec(),
            columns[11]
                .split(':')
                .map(|v| v.parse::<u64>().unwrap())
                .collect::<Vec<_>>(),
            "{context}"
        );
        assert_eq!(receipt.destinations, destinations(columns[12]), "{context}");
        assert_eq!(state.slots, grant_slots(columns[18]), "{context}");
        if let Some(stored) = &receipt.stored {
            let last: Vec<u64> = columns[13].split(':').map(|v| v.parse().unwrap()).collect();
            assert_eq!(
                (
                    stored.last_slot as u64,
                    stored.last.id,
                    stored.last_count as u64
                ),
                (last[0], last[1], last[2]),
                "{context}"
            );
            assert_eq!(stored.before_slots, before.slots);
            assert_eq!(stored.before_next_item, before.next_item);
            assert_eq!(stored.after_next_item, state.next_item);
            assert_eq!(stored.after_slots, state.slots);
        } else {
            assert_eq!(state, before);
            assert_eq!(columns[13], "-");
        }
        receipt.validate(&catalog, &state.equipped).unwrap();
        cases += 1;
    }
    assert_eq!(cases, 1024);
}
#[test]
fn grants_fail_closed_without_filtering_metadata_or_mutating_state() {
    for field in 0..7 {
        let mut row = item(70001, 1, 0);
        match field {
            0 => row.random_property = 7,
            1 => row.duration = 1,
            2 => row.script_id = 8,
            3 => row.bag_family = 9,
            4 => row.map = 1,
            5 => row.area = 1,
            _ => {
                row.inventory_type = 18;
                row.container_slots = 37;
            }
        }
        let catalog = Catalog::new(vec![row.clone()], audit()).unwrap();
        assert_eq!(catalog.item(row.entry).unwrap(), &row);
        assert_eq!(catalog.items().count(), 1);
        let mut state = State::new(BTreeMap::new(), 1).unwrap();
        let before = state.clone();
        assert!(state
            .grant_item(
                &catalog,
                row.entry,
                1,
                GrantTarget::SelfPlayer,
                |_, _, _| panic!("unsupported callback")
            )
            .is_err());
        assert_eq!(state, before);
    }
}
#[test]
fn empty_carried_bag_preserves_metadata_and_rejects_restore_tampering() {
    let mut row = item(70001, 1, 0);
    row.inventory_type = 18;
    row.class = 1;
    row.container_slots = 36;
    row.bonding = 1;
    row.spells[0] = Spell {
        id: 8,
        trigger: 0,
        charges: -5,
        ppm_rate_bits: 1.25f32.to_bits(),
        cooldown: 10,
        category: 2,
        category_cooldown: 20,
    };
    row.item_set = 3;
    row.start_quest = 4;
    let catalog = Catalog::new(vec![row.clone()], audit()).unwrap();
    let mut altered = row.clone();
    altered.spells[0].ppm_rate_bits = 2.5f32.to_bits();
    assert_ne!(
        catalog.hash(),
        Catalog::new(vec![altered], audit()).unwrap().hash()
    );
    let mut state = State::new(BTreeMap::new(), 1).unwrap();
    let grant = state
        .grant_item(&catalog, 70001, 20, GrantTarget::SelfPlayer, |_, _, _| {
            Ok(())
        })
        .unwrap();
    assert_eq!((grant.granted, grant.ungranted), (16, 4));
    assert_eq!(state.slots.len(), 16);
    for item in state.slots.values() {
        assert_eq!(item.kind, InstanceKind::EmptyBag { slots: 36 });
        assert_eq!(item.charges, [0; 5]);
        assert!(!item.bound);
    }
    let restored: State = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    restored.validate(&catalog).unwrap();
    assert_eq!(restored, state);
    assert_eq!(catalog.item(70001).unwrap(), &row);
    for field in 0..4 {
        let mut bad = state.clone();
        let item = bad.slots.get_mut(&23).unwrap();
        match field {
            0 => item.kind = InstanceKind::Item,
            1 => item.charges[0] = -5,
            2 => item.random_property_id = 1,
            _ => item.enchantments[0] = 9,
        }
        assert!(bad.validate(&catalog).is_err());
    }
    let before = state.clone();
    let zero = state
        .grant_item(&catalog, 70001, 1, GrantTarget::SelfPlayer, |_, _, _| {
            panic!("full callback")
        })
        .unwrap();
    assert_eq!(zero.granted, 0);
    assert_eq!(state, before);
    assert!(state
        .store_new(&catalog, 70001, 1, |_, _, _| Ok(()))
        .is_err());
}
#[test]
fn partial_grant_rollback_and_receipt_tamper_are_checked() {
    let catalog = catalog(1, 0);
    let mut state = State::new(BTreeMap::new(), 99).unwrap();
    let before = state.clone();
    assert!(state
        .grant_item(&catalog, 70001, 20, GrantTarget::SelfPlayer, |_, _, _| Err(
            Error::Callback("fail")
        ))
        .is_err());
    assert_eq!(state, before);
    state.next_item = u64::MAX - 1;
    let before = state.clone();
    assert_eq!(
        state.grant_item(
            &catalog,
            70001,
            20,
            GrantTarget::SelfPlayer,
            |_, _, _| panic!("overflow callback")
        ),
        Err(Error::Exhausted)
    );
    assert_eq!(state, before);
    state.next_item = 99;
    let grant = state
        .grant_item(&catalog, 70001, 20, GrantTarget::SelfPlayer, |_, _, _| {
            Ok(())
        })
        .unwrap();
    for field in 0..5 {
        let mut bad = grant.clone();
        match field {
            0 => bad.requested += 1,
            1 => bad.granted += 1,
            2 => bad.ungranted += 1,
            3 => bad.destinations[0].count += 1,
            _ => bad.stored.as_mut().unwrap().after_next_item += 1,
        }
        assert!(bad.validate(&catalog, &state.equipped).is_err());
    }
}

#[test]
fn zero_grant_receipt_recomputes_unique_vs_capacity_refusal() {
    let catalog = catalog(1, 1);
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
    let zero = state
        .grant_item(
            &catalog,
            70001,
            1,
            GrantTarget::SelfPlayer,
            |_, _, _| Ok(()),
        )
        .unwrap();
    zero.validate(&catalog, &state.equipped).unwrap();
    let mut bad = zero;
    bad.storage_result = InventoryResult::InventoryFull;
    assert!(bad.validate(&catalog, &state.equipped).is_err());
}
