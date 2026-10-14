// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_runtime::native::{self as n, synthetic_quest_fixture as quests};
#[test]
fn uncertain_admin_publication_points_withhold_receipts_and_reconcile_exact_outcome() {
    use super::super::super::storage::SaveFault;
    for fault in [
        SaveFault::AfterPreviousRename,
        SaveFault::AfterRename,
        SaveFault::BeforeVerification,
    ] {
        let t = Temp::new();
        let mut profile = quests::profile();
        profile.admin = Some(n::AdminPolicy {
            world: [1; 16],
            world_owner: f::player().owner,
            allowlist: Default::default(),
            revision: 1,
        });
        let mut world =
            n::NativeWorld::synthetic_component_test(profile.clone(), [1; 16], f::player(), 42)
                .unwrap();
        let identity = Identity::of(&world.checkpoint().unwrap());
        let (mut store, _) = NativeStore::create(t.path(), &world).unwrap();
        let request = n::AdminRequest {
            key: [7; 16],
            world: [1; 16],
            player: f::player(),
            policy_revision: 1,
            operation: n::AdminOperation::GainOneLevel,
        };
        let input = quests::command(&world, n::Command::Admin(request.clone()));
        world
            .step(1, Some(input), &mut quests::QuestAuthority::default())
            .unwrap();
        let computed = world.admin_receipt(&request.key).unwrap().clone();
        assert!(store.committed_admin_receipt(&request).is_none());
        store.storage.inject_fault(fault);
        assert!(store.checkpoint(&mut world).is_err());
        assert!(store.requires_reopen());
        assert!(store.committed_admin_receipt(&request).is_none());
        assert_eq!(
            store
                .committed_admin_snapshot(f::player().owner)
                .unwrap()
                .level,
            1
        );
        drop(store);
        let (mut store, mut reopened) = NativeStore::open(t.path(), identity, |c| {
            n::NativeWorld::restore_synthetic_component_test(profile.clone(), c)
        })
        .unwrap();
        if matches!(fault, SaveFault::AfterPreviousRename) {
            assert!(store.committed_admin_receipt(&request).is_none());
            assert_eq!(reopened.player_level(), 1);
        } else {
            let durable = store.committed_admin_receipt(&request).unwrap();
            assert_eq!(durable.receipt(), &computed);
            assert_eq!(
                durable.boundary(),
                &reopened.checkpoint().unwrap().identity()
            );
            assert_eq!(reopened.player_level(), 2);
            let calls = reopened
                .state()
                .quest_progression
                .as_ref()
                .unwrap()
                .derived_calls
                .clone();
            let retry = quests::command(&reopened, n::Command::Admin(request.clone()));
            reopened
                .step(
                    reopened.state().tick + 1,
                    Some(retry),
                    &mut quests::QuestAuthority::default(),
                )
                .unwrap();
            store.checkpoint(&mut reopened).unwrap();
            assert_eq!(reopened.player_level(), 2);
            assert_eq!(
                reopened
                    .state()
                    .quest_progression
                    .as_ref()
                    .unwrap()
                    .derived_calls,
                calls
            );
            assert_eq!(
                store.committed_admin_receipt(&request).unwrap().receipt(),
                &computed
            );
        }
    }
}

#[test]
fn item_grant_failed_publication_reopen_and_exact_retry_do_not_allocate_or_notify_twice() {
    use super::super::super::storage::SaveFault;
    for fault in [
        SaveFault::AfterPreviousRename,
        SaveFault::AfterRename,
        SaveFault::BeforeVerification,
    ] {
        let t = Temp::new();
        let mut profile = n::synthetic_loot_fixture::item_grant_profile();
        profile.admin = Some(n::AdminPolicy {
            world: [1; 16],
            world_owner: f::player().owner,
            allowlist: Default::default(),
            revision: 1,
        });
        let mut world =
            n::NativeWorld::synthetic_component_test(profile.clone(), [1; 16], f::player(), 42)
                .unwrap();
        let identity = Identity::of(&world.checkpoint().unwrap());
        let (mut store, _) = NativeStore::create(t.path(), &world).unwrap();
        let request = n::AdminRequest {
            key: [77; 16],
            world: [1; 16],
            player: f::player(),
            policy_revision: 1,
            operation: n::AdminOperation::GrantItem {
                item: 70003,
                count: 20,
            },
        };
        let input = quests::command(&world, n::Command::Admin(request.clone()));
        world
            .step(
                1,
                Some(input),
                &mut n::synthetic_loot_fixture::LootAuthority::default(),
            )
            .unwrap();
        let computed = world.admin_receipt(&request.key).unwrap().clone();
        assert!(
            matches!(&computed.effect,n::AdminEffect::ItemGrant(g) if g.granted==16&&g.ungranted==4&&g.storage_result.protocol()==50)
        );
        let inventory = world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .inventory
            .clone();
        let rng = world.state().rng_counter;
        assert!(store.committed_admin_receipt(&request).is_none());
        store.storage.inject_fault(fault);
        assert!(store.checkpoint(&mut world).is_err());
        assert!(store.committed_admin_receipt(&request).is_none());
        drop(store);
        let (mut store, mut reopened) = NativeStore::open(t.path(), identity, |c| {
            n::NativeWorld::restore_synthetic_component_test(profile.clone(), c)
        })
        .unwrap();
        if matches!(fault, SaveFault::AfterPreviousRename) {
            assert!(store.committed_admin_receipt(&request).is_none());
            assert!(reopened
                .state()
                .quest_progression
                .as_ref()
                .unwrap()
                .inventory
                .as_ref()
                .unwrap()
                .slots
                .is_empty());
        } else {
            assert_eq!(
                store.committed_admin_receipt(&request).unwrap().receipt(),
                &computed
            );
            assert_eq!(
                reopened
                    .state()
                    .quest_progression
                    .as_ref()
                    .unwrap()
                    .inventory,
                inventory
            );
            let retry = quests::command(&reopened, n::Command::Admin(request.clone()));
            reopened
                .step(
                    reopened.state().tick + 1,
                    Some(retry),
                    &mut n::synthetic_loot_fixture::LootAuthority::default(),
                )
                .unwrap();
            assert!(!reopened
                .last_tick()
                .unwrap()
                .events
                .iter()
                .any(|e| matches!(e, n::Event::InventoryStored(_) | n::Event::ItemAcquired(_))));
            store.checkpoint(&mut reopened).unwrap();
            assert_eq!(
                reopened
                    .state()
                    .quest_progression
                    .as_ref()
                    .unwrap()
                    .inventory,
                inventory
            );
            assert_eq!(reopened.state().rng_counter, rng);
            assert_eq!(
                store.committed_admin_receipt(&request).unwrap().receipt(),
                &computed
            );
        }
    }
}
