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
