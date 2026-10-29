// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored one-hop loot captures; neither public SQL nor synthetic order admits native data.
use super::*;
use benilla_sim::loot as l;

fn reference_setup() -> (
    Profile,
    benilla_sim::starter_init::StarterInput,
    QuestProgressionInput,
) {
    let (p, input, mut data) = fixed_setup();
    let loot = data.loot.as_mut().unwrap();
    let mut child = loot.captures[0].clone();
    // The two namespaces may legitimately use the same ID.
    child.source_fingerprint = [0xa1; 32];
    child.loader_order.ordered_result_fingerprint = [0xb2; 32];
    child.loader_order.result_row_count = 5;
    child.loader_order.selected_row_positions = vec![0, 2, 4];
    for entry in [70003, 70004] {
        let mut row = child.rows[0].clone();
        row.item = entry;
        row.group = 2;
        row.chance_or_quest_chance_bits = 0f32.to_bits();
        child.rows.push(row);
        let l::ItemLookup::Present(mut item) = child.items[&70001].clone() else {
            unreachable!()
        };
        item.entry = entry;
        child.items.insert(entry, l::ItemLookup::Present(item));
    }
    let body = &mut loot.captures[0];
    body.rows[0].item = 0;
    body.rows[0].chance_or_quest_chance_bits = 25f32.to_bits();
    body.rows[0].min_count_or_reference = -(child.loot_id as i32);
    body.rows[0].max_count = 2;
    body.items.clear();
    loot.reference_captures = vec![child];
    loot.rules.referenced_rate_bits = 4f32.to_bits();
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    let bag = items.iter_mut().find(|i| i.entry == 70001).unwrap();
    bag.class = 1;
    bag.inventory_type = 18;
    bag.container_slots = 6;
    bag.stackable = 1;
    bag.bonding = 1;
    items.extend([70003, 70004].map(synthetic_loot_fixture::item));
    loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
    (p, input, data)
}

fn reference_world() -> NativeWorld {
    let (p, input, data) = reference_setup();
    world_from(p, input, data)
}

fn loot_draw(draw: &Draw) -> bool {
    matches!(
        draw.purpose,
        DrawPurpose::Loot { .. } | DrawPurpose::LootChance { .. } | DrawPurpose::LootShuffle { .. }
    )
}

#[test]
fn reference_corpse_collects_repeated_bags_and_groups_once_with_replay() {
    let mut w = reference_world();
    let initial = w.checkpoint().unwrap();
    let mut authority = LootAuthority::default();
    kill(&mut w, &mut authority);
    let id = w.state.npcs[&2].id;
    let generated = w.state.npcs[&2].corpse_loot.clone().unwrap();
    assert_eq!(generated.generated.items.len(), 4);
    assert_eq!(generated.generated.items[0].item, 70001);
    assert_eq!(generated.generated.items[2].item, 70001);
    assert!(generated.draws.len() <= 4096);
    assert!(generated
        .draws
        .iter()
        .any(|d| matches!(d.purpose, DrawPurpose::LootShuffle { .. })));
    assert!(matches!(
        generated.draws[0].purpose,
        DrawPurpose::LootChance { .. }
    ));
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 0);
    let money = progress(&w).money;
    step(&mut w, &mut authority, Command::OpenLoot { corpse: id });
    for item in &generated.generated.items {
        step(
            &mut w,
            &mut authority,
            Command::LootItem {
                corpse: id,
                slot: item.slot,
            },
        );
        assert!(!w.last_tick().unwrap().draws.iter().any(loot_draw));
    }
    step(&mut w, &mut authority, Command::LootMoney { corpse: id });
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 2);
    let bags = inventory(&w)
        .slots
        .values()
        .filter(|item| item.item == 70001)
        .collect::<Vec<_>>();
    assert_eq!(bags.len(), 2);
    assert_ne!(bags[0].id, bags[1].id);
    assert!(bags.iter().all(|item| item.bound
        && item.count == 1
        && item.kind == inv::InstanceKind::EmptyBag { slots: 6 }));
    assert_eq!(progress(&w).money, money + generated.generated.money);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let expected = inventory(&w).clone();
    w = roundtrip(&w);
    step(&mut w, &mut authority, Command::OpenLoot { corpse: id });
    for slot in 0..4 {
        step(
            &mut w,
            &mut authority,
            Command::LootItem { corpse: id, slot },
        );
    }
    assert_eq!(inventory(&w), &expected);
    assert_eq!(
        w.state.npcs[&2].corpse_loot.as_ref().unwrap().draws,
        generated.draws
    );
    roundtrip(&w);
}

fn death_boundary() -> (NativeWorld, Option<OwnedInput>, NativeWorld) {
    let mut w = reference_world();
    let mut authority = LootAuthority::default();
    for tick in 0..31 {
        let input = (tick == 0).then(|| {
            command(
                &w,
                Command::Cast {
                    ability: 60001,
                    target: w.state.npcs[&2].id,
                },
            )
        });
        let before = w.clone();
        w.step(w.state.tick + 1, input.clone(), &mut authority)
            .unwrap();
        if w.state.npcs[&2].health == 0 {
            assert!(w.last_tick().unwrap().draws.iter().any(loot_draw));
            return (before, input, w);
        }
    }
    panic!("authored reference target did not die")
}

#[test]
fn reference_death_late_journal_failure_rolls_back_draws_and_retries_identically() {
    let (mut w, input, clean) = death_boundary();
    let before = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    let journal_bytes = w.journal_bytes;
    // journal_size executes after corpse generation and every frame callback.
    w.journal_bytes = MAX_BYTES;
    for _ in 0..2 {
        assert!(matches!(
            w.step(
                w.state.tick + 1,
                input.clone(),
                &mut LootAuthority::default()
            ),
            Err(NativeError::JournalFull | NativeError::TooLarge)
        ));
        assert_eq!(w.checkpoint().unwrap(), before);
        assert_eq!(w.records(), records);
        assert!(w.state.npcs[&2].corpse_loot.is_none());
    }
    w.journal_bytes = journal_bytes;
    w.step(w.state.tick + 1, input, &mut LootAuthority::default())
        .unwrap();
    assert_eq!(w.checkpoint().unwrap(), clean.checkpoint().unwrap());
    assert_eq!(w.records(), clean.records());
}

#[test]
fn reference_uncommitted_checkpoint_payload_retry_and_compaction_do_not_reroll() {
    let mut w = reference_world();
    let stale = w.checkpoint().unwrap();
    kill(&mut w, &mut LootAuthority::default());
    let checkpoint = w.checkpoint().unwrap();
    let encoded = checkpoint.encode().unwrap();
    let records = w.records().to_vec();
    let corpse = w.state.npcs[&2].corpse_loot.clone().unwrap();
    // Retry an uncommitted immutable payload; filesystem failure injection is a store test.
    for _ in 0..2 {
        assert_eq!(w.checkpoint().unwrap().encode().unwrap(), encoded);
        assert!(w.checkpoint_committed(&stale).is_err());
        assert_eq!(w.records(), records);
        assert_eq!(w.state.npcs[&2].corpse_loot.as_ref(), Some(&corpse));
    }
    w.checkpoint_committed(&checkpoint).unwrap();
    assert!(w.records().is_empty());
    w = roundtrip(&w);
    assert_eq!(w.state.npcs[&2].corpse_loot.as_ref(), Some(&corpse));
    let id = w.state.npcs[&2].id;
    step(
        &mut w,
        &mut LootAuthority::default(),
        Command::OpenLoot { corpse: id },
    );
    step(
        &mut w,
        &mut LootAuthority::default(),
        Command::ReleaseLoot { corpse: id },
    );
    assert!(!w.records().iter().flat_map(|r| &r.draws).any(loot_draw));
    assert_eq!(
        w.state.npcs[&2].corpse_loot.as_ref().unwrap().generated,
        corpse.generated
    );
    roundtrip(&w);
}

#[test]
fn reference_full_inventory_collection_preserves_corpse_and_all_item_state() {
    let mut w = reference_world();
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
    let progression = w.state.quest_progression.as_mut().unwrap();
    let inventory = progression.inventory.as_mut().unwrap();
    let remaining = 16 - inventory.slots.len() as u32;
    inventory
        .store_corpse_item(&catalog, 70001, remaining, |_, _, _| Ok(()))
        .unwrap();
    super::super::loot::update_inventory_receipts(progression, [0x75; 32]).unwrap();
    let mut authority = LootAuthority::default();
    kill(&mut w, &mut authority);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut authority, Command::OpenLoot { corpse: id });
    let corpse = w.state.npcs[&2].corpse_loot.clone();
    let expected_inventory = super::inventory(&w).clone();
    let receipts = progress(&w).inventory_receipts.clone();
    let quests = progress(&w).quests.snapshot();
    for _ in 0..2 {
        step(
            &mut w,
            &mut authority,
            Command::LootItem {
                corpse: id,
                slot: 0,
            },
        );
        assert_eq!(super::inventory(&w), &expected_inventory);
        assert_eq!(progress(&w).inventory_receipts, receipts);
        assert_eq!(progress(&w).quests.snapshot(), quests);
        assert_eq!(w.state.npcs[&2].corpse_loot, corpse);
        assert!(w.last_tick().unwrap().events.iter().any(|event| matches!(
            event,
            Event::Loot(LootEvent::StorageFailed {
                result: inv::InventoryResult::InventoryFull,
                ..
            })
        )));
        assert!(!w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(event, Event::InventoryStored(_) | Event::ItemAcquired(_))));
        assert!(!w.last_tick().unwrap().draws.iter().any(loot_draw));
    }
    roundtrip(&w);
}

#[test]
fn reference_inventory_metadata_closure_covers_every_child_and_shared_item() {
    let (mut p, input, mut data) = reference_setup();
    let loot = data.loot.as_mut().unwrap();
    loot.reference_captures[0].loader_order.result_row_count = 10;
    let mut child = loot.reference_captures[0].clone();
    child.loot_id += 1;
    child.loader_order.selected_row_positions = vec![5, 7, 9];
    let body = &mut loot.captures[0];
    body.rows.push(l::Row {
        min_count_or_reference: -(child.loot_id as i32),
        ..body.rows[0].clone()
    });
    body.rows.push(child.rows[0].clone());
    body.items.insert(70001, child.items[&70001].clone());
    body.loader_order.result_row_count = 3;
    body.loader_order.selected_row_positions = vec![0, 1, 2];
    loot.reference_captures.push(child);
    // Identical shared metadata is legal across both namespaces and multiple references.
    p.initialize_quest_progression(&input, data).unwrap();
    for case in 0..5 {
        let (mut p, input, mut data) = reference_setup();
        let loot = data.loot.as_mut().unwrap();
        match case {
            0 => {
                let items = loot
                    .inventory
                    .items()
                    .filter(|i| i.entry != 70004)
                    .cloned()
                    .collect();
                loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
            }
            1 => {
                let l::ItemLookup::Present(item) =
                    loot.reference_captures[0].items.get_mut(&70004).unwrap()
                else {
                    unreachable!()
                };
                item.display_id += 1;
            }
            2 | 3 => {
                loot.reference_captures[0].loader_order.result_row_count = 10;
                let mut child = loot.reference_captures[0].clone();
                child.loot_id += 1;
                child.loader_order.selected_row_positions = vec![5, 7, 9];
                let body = &mut loot.captures[0];
                body.rows.push(l::Row {
                    min_count_or_reference: -(child.loot_id as i32),
                    ..body.rows[0].clone()
                });
                body.loader_order.result_row_count = 2;
                body.loader_order.selected_row_positions.push(1);
                loot.reference_captures.push(child);
                l::Catalog::new_with_references(
                    loot.captures.clone(),
                    loot.reference_captures.clone(),
                )
                .unwrap();
                let child = &mut loot.reference_captures[1];
                if case == 2 {
                    let l::ItemLookup::Present(item) = child.items.get_mut(&70001).unwrap() else {
                        unreachable!()
                    };
                    item.quality += 1;
                } else {
                    let l::ItemLookup::Present(item) = child.items.get_mut(&70003).unwrap() else {
                        unreachable!()
                    };
                    item.flags = 0x800;
                }
            }
            _ => {
                let mut leaf = loot.reference_captures[0].rows[0].clone();
                leaf.item = 70003;
                let mut item = loot.reference_captures[0].items[&70003].clone();
                let l::ItemLookup::Present(metadata) = &mut item else {
                    unreachable!()
                };
                metadata.display_id += 1;
                let body = &mut loot.captures[0];
                body.rows.push(leaf);
                body.items.insert(70003, item);
                body.loader_order.result_row_count = 2;
                body.loader_order.selected_row_positions.push(1);
            }
        }
        // Each child still passes row/order admission in isolation; metadata closure fails.
        for child in &loot.reference_captures {
            l::Catalog::new(vec![child.clone()]).unwrap();
        }
        let expected = match case {
            0 => NativeError::Inventory(inv::Error::Invalid("unknown_item")),
            1 => NativeError::InvalidProfile,
            _ => NativeError::Loot(l::Error::Invalid("inconsistent item closure")),
        };
        assert_eq!(
            p.initialize_quest_progression(&input, data),
            Err(expected),
            "case {case}"
        );
    }
}

#[test]
fn reference_rate_captures_and_loader_order_are_immutable_restore_inputs() {
    let mut w = reference_world();
    kill(&mut w, &mut LootAuthority::default());
    let checkpoint = w.checkpoint().unwrap();
    for case in 0..5 {
        let mut profile = w.profile.clone();
        let genesis = profile
            .quest_progression
            .as_mut()
            .unwrap()
            .loot
            .as_mut()
            .unwrap();
        if case == 0 {
            genesis.rules.referenced_rate_bits = 2f32.to_bits();
        } else {
            let body = genesis.catalog.captures().cloned().collect();
            let mut references: Vec<_> = genesis.catalog.reference_captures().cloned().collect();
            match case {
                1 => references[0].source_fingerprint = [0xc3; 32],
                2 => references[0].loader_order.ordered_result_fingerprint = [0xd4; 32],
                3 => references[0].loader_order.selected_row_positions = vec![0, 1, 4],
                _ => references[0].rows.swap(1, 2),
            }
            genesis.catalog = l::Catalog::new_with_references(body, references).unwrap();
        }
        // Serialization retains the committed identity; validation must reconstruct it.
        assert_eq!(profile.digest(), w.profile.digest());
        assert!(
            NativeWorld::restore_synthetic_component_test(profile, checkpoint.clone()).is_err(),
            "case {case}"
        );
    }
    let baseline = w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .identity();
    for case in 0..5 {
        let (mut p, input, mut data) = reference_setup();
        let loot = data.loot.as_mut().unwrap();
        match case {
            0 => loot.rules.referenced_rate_bits = 2f32.to_bits(),
            1 => loot.reference_captures[0].source_fingerprint = [0xc3; 32],
            2 => {
                loot.reference_captures[0]
                    .loader_order
                    .ordered_result_fingerprint = [0xd4; 32]
            }
            3 => {
                loot.reference_captures[0]
                    .loader_order
                    .selected_row_positions = vec![0, 1, 4]
            }
            _ => loot.reference_captures[0].rows.swap(1, 2),
        }
        p.initialize_quest_progression(&input, data).unwrap();
        assert_ne!(
            p.quest_progression
                .as_ref()
                .unwrap()
                .loot
                .as_ref()
                .unwrap()
                .identity(),
            baseline
        );
        assert!(NativeWorld::restore_synthetic_component_test(p, checkpoint.clone()).is_err());
    }
}

#[test]
fn reference_generation_and_draw_witness_tampering_reject_after_outer_rehash() {
    let mut w = reference_world();
    kill(&mut w, &mut LootAuthority::default());
    let checkpoint = w.checkpoint().unwrap();
    for case in 0..5 {
        let mut value = serde_json::to_value(&checkpoint).unwrap();
        let corpse = &mut value["state"]["npcs"]["2"]["corpse_loot"];
        match case {
            0 => corpse["generated"]["items"][0]["count"] = 2.into(),
            1 => corpse["generated"]["items"][0]["display_id"] = 1.into(),
            2 => corpse["draws"][0]["value"] = 999.into(),
            3 => corpse["draws"] = serde_json::json!([]),
            _ => corpse["first_counter"] = 0.into(),
        }
        let mut forged: Checkpoint = serde_json::from_value(value).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v71\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v71\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err(),
            "case {case}"
        );
    }
}
