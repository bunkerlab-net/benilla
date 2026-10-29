// SPDX-License-Identifier: GPL-3.0-or-later
use super::synthetic_fixture::player;
use super::synthetic_loot_fixture as fixture;
use super::synthetic_quest_fixture::command;
use super::*;
use benilla_sim::{inventory as inv, quest_lifecycle as q};
fn profile() -> Profile {
    let mut p = fixture::item_grant_profile();
    p.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    p
}
fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], player(), 42).unwrap()
}
fn request(w: &NativeWorld, key: u8, item: u32, count: u32) -> AdminRequest {
    AdminRequest {
        key: [key; 16],
        world: w.state.world,
        player: w.state.player,
        policy_revision: 7,
        operation: AdminOperation::GrantItem { item, count },
    }
}
fn apply(w: &mut NativeWorld, r: &AdminRequest) {
    let input = command(w, Command::Admin(r.clone()));
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
}
fn inventory(w: &NativeWorld) -> &inv::State {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
}
#[test]
fn grants_without_corpse_commit_partial_receipts_and_retry_exactly() {
    let mut w = world();
    let genesis = w.checkpoint().unwrap();
    let r = request(&w, 1, 70001, 20);
    apply(&mut w, &r);
    let receipt = w.admin_receipt(&r.key).unwrap().clone();
    let AdminEffect::ItemGrant(g) = &receipt.effect else {
        panic!("item receipt")
    };
    assert_eq!((g.requested, g.granted, g.ungranted), (20, 7, 13));
    assert_eq!(g.storage_result, inv::InventoryResult::CantCarryMoreOfThis);
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 7);
    assert!(w.state.npcs.values().all(|n| n.corpse_loot.is_none()));
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::ItemAcquired(ItemAcquisition::AdminSelf))));
    let before = inventory(&w).clone();
    let rng = w.state.rng_counter;
    apply(&mut w, &r);
    assert_eq!(inventory(&w), &before);
    assert_eq!(w.state.rng_counter, rng);
    assert_eq!(w.admin_receipt(&r.key), Some(&receipt));
    assert!(!w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::InventoryStored(_) | Event::ItemAcquired(_))));
    let zero = request(&w, 2, 70001, 1);
    apply(&mut w, &zero);
    assert!(
        matches!(&w.admin_receipt(&zero.key).unwrap().effect,AdminEffect::ItemGrant(g) if g.granted==0&&g.ungranted==1&&g.stored.is_none())
    );
    let bag = request(&w, 3, 70003, 20);
    apply(&mut w, &bag);
    assert!(
        matches!(&w.admin_receipt(&bag.key).unwrap().effect,AdminEffect::ItemGrant(g) if g.granted==14&&g.ungranted==6)
    );
    assert!(inventory(&w)
        .slots
        .values()
        .filter(|i| i.item == 70003)
        .all(|i| i.kind == inv::InstanceKind::EmptyBag { slots: 16 }
            && !i.bound
            && i.charges == [0; 5]));
    let final_checkpoint = w.checkpoint().unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(profile(), final_checkpoint).unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    let mut replay = NativeWorld::restore_synthetic_component_test(profile(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn grant_quest_credit_precedes_storage_and_does_not_autoaccept_item_quest() {
    let (mut p, input, mut rows) = fixture::item_grant_setup();
    let quest = &mut rows.quests[0];
    quest.required_creature = [0; 4];
    quest.required_creature_count = [0; 4];
    quest.required_item = [70004, 0, 0, 0];
    quest.required_item_count = [2, 0, 0, 0];
    quest.special_flags = q::DELIVER;
    p.initialize_quest_progression(&input, rows).unwrap();
    p.admin = profile().admin;
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
    let input = command(
        &w,
        Command::AcceptQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(1, Some(input), &mut fixture::LootAuthority::default())
        .unwrap();
    let r = request(&w, 1, 70004, 2);
    apply(&mut w, &r);
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
    let i = inventory(&w)
        .slots
        .values()
        .find(|i| i.item == 70004)
        .unwrap();
    assert_eq!(i.charges[0], -5);
    assert!(w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .quests
        .progress(70002)
        .is_none());
}
#[test]
fn catalog_visibility_does_not_enable_unsupported_grants_or_wrong_context() {
    let mut w = world();
    let s = w.admin_snapshot(player().owner).unwrap();
    assert!(s.items.iter().any(|i| i.item == 70005 && i.grant.is_err()));
    assert!(s.items.iter().any(|i| i.item == 70003 && i.grant.is_ok()));
    for (item, count, reason) in [
        (70005, 1, AdminRejection::UnsupportedItemGrant),
        (999, 1, AdminRejection::InvalidItemGrant),
        (70003, 0, AdminRejection::InvalidItemGrant),
        (70003, 10001, AdminRejection::InvalidItemGrant),
    ] {
        let r = request(&w, 1, item, count);
        assert_eq!(w.validate_admin_request(player().owner, &r), Err(reason));
    }
    let r = request(&w, 2, 70003, 1);
    let mut input = command(&w, Command::Admin(r.clone()));
    input.authenticated_owner = [99; 16];
    let before = w.state_digest();
    assert!(w
        .step(1, Some(input), &mut fixture::LootAuthority::default())
        .is_err());
    assert_eq!(w.state_digest(), before);
    let mut wrong = r.clone();
    wrong.player.character = [88; 16];
    assert_eq!(
        w.validate_admin_request(player().owner, &wrong),
        Err(AdminRejection::WrongContext)
    );
    apply(&mut w, &r);
    let mut conflict = r;
    conflict.operation = AdminOperation::GrantItem {
        item: 70004,
        count: 1,
    };
    assert_eq!(
        w.validate_admin_request(player().owner, &conflict),
        Err(AdminRejection::RequestConflict)
    );
}
#[test]
fn grant_instance_and_receipt_tampering_refuses_restore() {
    let mut w = world();
    let r = request(&w, 1, 70003, 1);
    apply(&mut w, &r);
    for field in 0..5 {
        let mut bad = w.clone();
        let q = bad.state.quest_progression.as_mut().unwrap();
        match field {
            0 => {
                q.inventory
                    .as_mut()
                    .unwrap()
                    .slots
                    .get_mut(&23)
                    .unwrap()
                    .kind = inv::InstanceKind::Item
            }
            1 => {
                q.inventory
                    .as_mut()
                    .unwrap()
                    .slots
                    .get_mut(&23)
                    .unwrap()
                    .bound = true
            }
            2 => {
                q.inventory
                    .as_mut()
                    .unwrap()
                    .slots
                    .get_mut(&23)
                    .unwrap()
                    .random_property_id = 9
            }
            3 => {
                q.inventory
                    .as_mut()
                    .unwrap()
                    .slots
                    .get_mut(&23)
                    .unwrap()
                    .charges[0] = -5
            }
            _ => q.inventory.as_mut().unwrap().next_item = 1,
        }
        assert!(bad
            .checkpoint()
            .and_then(|c| NativeWorld::restore_synthetic_component_test(profile(), c))
            .is_err());
    }
}

#[test]
fn visible_grant_only_rows_do_not_widen_ordinary_reward_admission() {
    for item in [70003, 70004, 70005] {
        for choice in [false, true] {
            let (mut p, input, mut rows) = fixture::item_grant_setup();
            if choice {
                rows.quests[0].excluded.reward_choice_item[0] = item;
                rows.quests[0].excluded.reward_choice_count[0] = 1;
            } else {
                rows.quests[0].excluded.reward_item[0] = item;
                rows.quests[0].excluded.reward_item_count[0] = 1;
            }
            assert!(p.initialize_quest_progression(&input, rows).is_err());
        }
    }
}

#[test]
fn incremental_receipt_bytes_match_fresh_serialization_after_mixed_requests_and_reopen() {
    let mut w = world();
    let operations = [
        (
            17,
            AdminOperation::GrantItem {
                item: 70001,
                count: 3,
            },
        ),
        (2, AdminOperation::SetXpGain { enabled: false }),
        (
            9,
            AdminOperation::GrantItem {
                item: 70003,
                count: 1,
            },
        ),
        (3, AdminOperation::GainOneLevel),
        (
            21,
            AdminOperation::GrantItem {
                item: 70001,
                count: 20,
            },
        ),
        (
            6,
            AdminOperation::GrantItem {
                item: 70001,
                count: 1,
            },
        ),
        (31, AdminOperation::SetXpGain { enabled: true }),
    ];
    for (index, (key, operation)) in operations.into_iter().enumerate() {
        let mut r = request(&w, key, 70001, 1);
        r.operation = operation;
        apply(&mut w, &r);
        assert_eq!(w.admin_receipt(&r.key).unwrap().request, r);
        let admin = serde_json::to_value(&w.state.admin).unwrap();
        assert_eq!(admin["receipts"].as_array().unwrap().len(), index + 1);
        let freshly_serialized = serde_json::to_vec(&admin["receipts"]).unwrap();
        assert_eq!(
            admin["receipt_bytes"].as_u64().unwrap(),
            freshly_serialized.len() as u64,
            "request {key}: sorted insertion, array delimiters and separators"
        );
        let state_digest = w.state_digest();
        w = NativeWorld::restore_synthetic_component_test(
            profile(),
            Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(w.state_digest(), state_digest);
        assert_eq!(serde_json::to_value(&w.state.admin).unwrap(), admin);
    }
}

#[test]
fn altered_receipt_byte_cache_fails_restore_even_with_resealed_checkpoint() {
    let mut w = world();
    for (key, item, count) in [(3, 70001, 3), (1, 70003, 1), (2, 70001, 20)] {
        let r = request(&w, key, item, count);
        apply(&mut w, &r);
    }
    let checkpoint = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    let actual = checkpoint["state"]["admin"]["receipt_bytes"]
        .as_u64()
        .unwrap();
    for cache in [0, 2, actual - 1, actual + 1, u64::from(u32::MAX)] {
        let mut forged = checkpoint.clone();
        forged["state"]["admin"]["receipt_bytes"] = cache.into();
        let state: super::persistence::StateCheckpoint =
            serde_json::from_value(forged["state"].clone()).unwrap();
        let digest = crate::digest(b"benilla-native-state-v69\0", &state);
        forged["digest"] = serde_json::to_value(digest).unwrap();
        let tip: crate::Digest = serde_json::from_value(forged["history_tip"].clone()).unwrap();
        forged["checksum"] = serde_json::to_value(crate::digest(
            b"benilla-native-checkpoint-v69\0",
            &(state, digest, tip),
        ))
        .unwrap();
        let decoded: Checkpoint = serde_json::from_value(forged).unwrap();
        let decoded = Checkpoint::decode(&decoded.encode().unwrap()).unwrap();
        assert!(
            NativeWorld::restore_synthetic_component_test(profile(), decoded).is_err(),
            "cached {cache}, actual {actual}"
        );
    }
}

#[test]
fn previous_admin_and_progression_formats_cannot_decode_as_item_grants() {
    let w = world();
    let checkpoint = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    for version in [16, 17] {
        let mut old = checkpoint.clone();
        old["version"] = version.into();
        old["state"]["version"] = version.into();
        old["state"]["policy"] = if version == 16 {
            "benilla-native-admin-progression-v16"
        } else {
            "benilla-native-unspent-progression-prefix-v17"
        }
        .into();
        assert!(Checkpoint::decode(&serde_json::to_vec(&old).unwrap()).is_err());
    }
    assert_eq!(NATIVE_FORMAT_VERSION, 69);
}

#[test]
fn item_ledger_byte_bound_composes_with_level_sixty_history_and_exact_retries() {
    let (mut p, input, mut rows) =
        super::synthetic_quest_fixture::setup_with_levels(60, 60, false, 1.0f32.to_bits());
    rows.loot = fixture::item_grant_setup().2.loot;
    p.initialize_quest_progression(&input, rows).unwrap();
    p.admin = profile().admin;
    let mut w = NativeWorld::synthetic_component_test(p.clone(), [1; 16], player(), 42).unwrap();
    let first = request(&w, 1, 70003, 20);
    apply(&mut w, &first);
    let first_receipt = w.admin_receipt(&first.key).unwrap().clone();
    for key in 2..61 {
        let mut r = request(&w, key, 70001, 1);
        r.operation = AdminOperation::GainOneLevel;
        apply(&mut w, &r);
        assert_eq!(w.admin_receipt(&r.key).unwrap().request, r);
    }
    assert_eq!(w.player_level(), 60);
    let mut count = 60;
    while w.admin_snapshot(player().owner).unwrap().receipts_remaining != 0 {
        let mut r = request(&w, 200, 70003, 1);
        r.key[..8].copy_from_slice(&(count as u64).to_le_bytes());
        apply(&mut w, &r);
        assert!(matches!(
            &w.admin_receipt(&r.key).unwrap().effect,
            AdminEffect::ItemGrant(g) if g.granted == 0 && g.ungranted == 1
        ));
        count += 1;
        assert!(count < MAX_ADMIN_RECEIPTS);
    }
    let admin = serde_json::to_value(&w.state.admin).unwrap();
    let bytes = serde_json::to_vec(&admin["receipts"]).unwrap().len();
    assert_eq!(bytes as u64, admin["receipt_bytes"].as_u64().unwrap());
    assert!(bytes + MAX_ADMIN_SINGLE_RECEIPT_BYTES + 1 > MAX_ADMIN_RECEIPT_BYTES);
    assert!(bytes <= MAX_ADMIN_RECEIPT_BYTES);
    let inventory_before = inventory(&w).clone();
    apply(&mut w, &first);
    assert_eq!(w.admin_receipt(&first.key), Some(&first_receipt));
    assert_eq!(inventory(&w), &inventory_before);
    w.step(
        w.state.tick + 1,
        None,
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
    let bytes = w.checkpoint().unwrap().encode().unwrap();
    assert!(bytes.len() < MAX_BYTES);
    let restored =
        NativeWorld::restore_synthetic_component_test(p, Checkpoint::decode(&bytes).unwrap())
            .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    assert_eq!(
        restored
            .admin_snapshot(player().owner)
            .unwrap()
            .receipts_remaining,
        0
    );
    assert_eq!(restored.admin_receipt(&first.key), Some(&first_receipt));
}
