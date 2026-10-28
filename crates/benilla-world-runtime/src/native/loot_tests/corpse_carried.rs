// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored carried-item cases; no whole-table/source content admission claim.
use super::*;

fn carried_setup(
    shape: u32,
) -> (
    Profile,
    benilla_sim::starter_init::StarterInput,
    QuestProgressionInput,
) {
    let (p, input, mut data) = fixed_setup();
    let loot = data.loot.as_mut().unwrap();
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    let item = items.iter_mut().find(|i| i.entry == 70001).unwrap();
    for spell in &mut item.spells {
        spell.cooldown = -1;
        spell.category_cooldown = -1;
    }
    item.bonding = 1;
    match shape {
        0 => {
            item.class = 4;
            item.inventory_type = 5;
            item.maximum_durability = 37;
        }
        1 => {
            item.spells[0].id = 81001;
            item.spells[0].charges = -1;
            item.spells[0].category_cooldown = 1000;
        }
        2..=6 => {
            item.class = 1;
            item.inventory_type = 18;
            item.container_slots = 6;
            item.stackable = 1;
        }
        _ => unreachable!(),
    }
    loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
    (p, input, data)
}
fn carried_world(shape: u32) -> NativeWorld {
    let (p, input, data) = carried_setup(shape);
    world_from(p, input, data)
}
fn collect(w: &mut NativeWorld, a: &mut impl Authority) -> crate::NpcId {
    kill(w, a);
    let id = w.state.npcs[&2].id;
    step(w, a, Command::OpenLoot { corpse: id });
    step(
        w,
        a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    id
}

#[test]
fn corpse_carried_metadata_collects_once_and_replays_across_compaction() {
    for shape in 0..7 {
        let mut w = carried_world(shape);
        let start = w.checkpoint().unwrap();
        let mut a = LootAuthority::default();
        let id = collect(&mut w, &mut a);
        let item = inventory(&w)
            .slots
            .values()
            .find(|i| i.item == 70001)
            .unwrap();
        assert!(item.bound);
        assert_eq!(
            item.kind,
            if shape >= 2 {
                inv::InstanceKind::EmptyBag { slots: 6 }
            } else {
                inv::InstanceKind::Item
            }
        );
        assert_eq!(item.charges[0], if shape == 1 { -1 } else { 0 });
        assert_eq!(item.durability, if shape == 0 { 37 } else { 0 });
        assert_eq!(item.maximum_durability, item.durability);
        let genesis = w
            .profile
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap();
        assert!(genesis.inventory.ordinary_loot_ready(70001).is_err());
        assert!(genesis.inventory.use_ready(70001).is_err());
        assert!(genesis.inventory.equip_ready(70001).is_err());
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
        let checkpoint = w.checkpoint().unwrap();
        w.checkpoint_committed(&checkpoint).unwrap();
        assert!(w.records().is_empty());
        let after = inventory(&w).clone();
        w = roundtrip(&w);
        step(
            &mut w,
            &mut a,
            Command::LootItem {
                corpse: id,
                slot: 0,
            },
        );
        assert_eq!(inventory(&w), &after);
        assert_eq!(inventory(&roundtrip(&w)), &after);
    }
}

#[test]
fn corpse_carried_quest_callback_runs_before_storage_but_rewards_stay_closed() {
    let (p, input, mut data) = carried_setup(1);
    let quest = &mut data.quests[0];
    quest.required_creature = [0; 4];
    quest.required_creature_count = [0; 4];
    quest.required_item = [70001, 0, 0, 0];
    quest.required_item_count = [1, 0, 0, 0];
    quest.special_flags = q::DELIVER;
    let mut w = world_from(p.clone(), input.clone(), data.clone());
    let mut a = LootAuthority::default();
    let giver = w.state.npcs[&3].id;
    step(
        &mut w,
        &mut a,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
    collect(&mut w, &mut a);
    let events = &w.last_tick().unwrap().events;
    let credit = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::ItemCredit { .. },
                    ..
                })
            )
        })
        .unwrap();
    let stored = events
        .iter()
        .position(|e| matches!(e, Event::InventoryStored(_)))
        .unwrap();
    assert!(credit < stored);
    assert_eq!(progress(&w).quests.progress(70001).unwrap().items[0], 1);
    data.quests[0].excluded.reward_item = [70001, 0, 0, 0];
    data.quests[0].excluded.reward_item_count = [1, 0, 0, 0];
    let mut p = p;
    assert!(p.initialize_quest_progression(&input, data).is_err());
}

#[test]
fn corpse_carried_full_inventory_and_unique_refuse_without_partial_store_or_capacity() {
    for unique in [false, true] {
        let (p, input, mut data) = carried_setup(2);
        if unique {
            let loot = data.loot.as_mut().unwrap();
            let mut items: Vec<_> = loot.inventory.items().cloned().collect();
            items
                .iter_mut()
                .find(|i| i.entry == 70001)
                .unwrap()
                .maximum_count = 1;
            loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
        }
        let mut w = world_from(p, input, data);
        let catalog = w
            .profile
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap()
            .inventory
            .clone();
        let current = w.state.quest_progression.as_mut().unwrap();
        let inv = current.inventory.as_mut().unwrap();
        let count = if unique {
            1
        } else {
            16 - inv.slots.len() as u32
        };
        inv.store_corpse_item(&catalog, 70001, count, |_, _, _| Ok(()))
            .unwrap();
        super::super::loot::update_inventory_receipts(current, [0x31; 32]).unwrap();
        let mut a = LootAuthority::default();
        kill(&mut w, &mut a);
        let id = w.state.npcs[&2].id;
        step(&mut w, &mut a, Command::OpenLoot { corpse: id });
        let inventory_before = inventory(&w).clone();
        let corpse_before = w.state.npcs[&2].corpse_loot.clone();
        let quests_before = progress(&w).quests.snapshot();
        step(
            &mut w,
            &mut a,
            Command::LootItem {
                corpse: id,
                slot: 0,
            },
        );
        assert_eq!(inventory(&w), &inventory_before);
        assert_eq!(w.state.npcs[&2].corpse_loot, corpse_before);
        assert_eq!(progress(&w).quests.snapshot(), quests_before);
        assert!(w.last_tick().unwrap().events.iter().any(|e|matches!(e,Event::Loot(LootEvent::StorageFailed{result,..}) if *result==if unique {inv::InventoryResult::CantCarryMoreOfThis}else{inv::InventoryResult::InventoryFull})));
        assert!(!w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::InventoryStored(_) | Event::ItemAcquired(_))));
        assert!(inventory(&w)
            .slots
            .keys()
            .all(|slot| (23..39).contains(slot)));
    }
}

#[test]
fn corpse_carried_callback_and_allocation_faults_roll_back_entire_frame() {
    for callback in [false, true] {
        let mut w = carried_world(1);
        let mut a = LootAuthority::default();
        kill(&mut w, &mut a);
        let id = w.state.npcs[&2].id;
        step(&mut w, &mut a, Command::OpenLoot { corpse: id });
        if callback {
            let state = w.state.quest_progression.as_mut().unwrap();
            let mut snapshot = state.quests.snapshot();
            snapshot.next_sequence = u64::MAX;
            let data = carried_setup(1).2;
            let catalog = q::Catalog::new(data.quests, data.relations, data.quest_audit).unwrap();
            state.quests = q::State::restore(&catalog, snapshot).unwrap();
        } else {
            w.state
                .quest_progression
                .as_mut()
                .unwrap()
                .inventory
                .as_mut()
                .unwrap()
                .next_item = u64::MAX;
        }
        let before = w.checkpoint().unwrap();
        let records = w.records().to_vec();
        let input = command(
            &w,
            Command::LootItem {
                corpse: id,
                slot: 0,
            },
        );
        let result = w.step(w.state.tick + 1, Some(input), &mut a);
        assert!(
            matches!(result, Err(NativeError::Inventory(inv::Error::Callback(_))))
                || !callback
                    && matches!(result, Err(NativeError::Inventory(inv::Error::Exhausted)))
        );
        assert_eq!(w.checkpoint().unwrap(), before);
        assert_eq!(w.records(), records);
    }
}

#[test]
fn corpse_carried_tampered_bound_charge_bag_and_receipt_reject_after_outer_rehash() {
    for shape in [0, 1, 2] {
        let mut w = carried_world(shape);
        collect(&mut w, &mut LootAuthority::default());
        let checkpoint = w.checkpoint().unwrap();
        let (slot, item) = inventory(&w)
            .slots
            .iter()
            .find(|(_, i)| i.item == 70001)
            .unwrap();
        for mutation in 0..4 {
            let mut value = serde_json::to_value(&checkpoint).unwrap();
            let instance =
                &mut value["state"]["quest_progression"]["inventory"]["slots"][slot.to_string()];
            match mutation {
                0 => instance["bound"] = false.into(),
                1 => instance["charges"][0] = 99.into(),
                2 => {
                    instance["kind"] =
                        serde_json::to_value(inv::InstanceKind::EmptyBag { slots: 12 }).unwrap()
                }
                _ => {
                    value["state"]["quest_progression"]["inventory_receipts"][item.id.to_string()]
                        ["reason"] = serde_json::to_value([9u8; 32]).unwrap()
                }
            }
            let mut forged: Checkpoint = serde_json::from_value(value).unwrap();
            forged.digest = crate::digest(b"benilla-native-state-v64\0", &forged.state);
            forged.checksum = crate::digest(
                b"benilla-native-checkpoint-v64\0",
                &(&forged.state, forged.digest, forged.history_tip),
            );
            assert!(
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err(),
                "shape {shape}, mutation {mutation}"
            );
        }
    }
}

#[test]
fn corpse_carried_wrong_owner_stale_corpse_and_missing_slot_cannot_acquire() {
    let mut w = carried_world(2);
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    let before = w.checkpoint().unwrap();
    let mut input = command(
        &w,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    input.authenticated_owner = [9; 16];
    assert_eq!(
        w.step(w.state.tick + 1, Some(input), &mut a),
        Err(NativeError::InvalidInput)
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    let before = inventory(&w).clone();
    let stale = crate::NpcId {
        spawn: id.spawn,
        generation: id.generation + 1,
    };
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: stale,
            slot: 0,
        },
    );
    assert_eq!(inventory(&w), &before);
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 99,
        },
    );
    assert_eq!(inventory(&w), &before);
    assert!(!w.state.npcs[&2]
        .corpse_loot
        .as_ref()
        .unwrap()
        .collected
        .contains(&0));
    assert!(progress(&w).loot_target.is_none());
}
