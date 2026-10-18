// SPDX-License-Identifier: GPL-3.0-or-later
//! Rehashed authored multi-neighbor evidence must retain its independent owner anchor.
use super::*;
use sha2::Sha256;

/// Exercise the real checkpoint boundary with both pending and historical payloads
/// changed together. Outer checksums alone must not admit forged source receipts.
pub(super) fn assert_mutations_rejected(world: &NativeWorld) {
    let original = world.checkpoint().unwrap();
    let base = serde_json::to_value(&original).unwrap();
    assert_eq!(life::rehash(base.clone()), original);
    let restored =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), original.clone())
            .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), original);
    for case in 0..8 {
        let mut value = base.clone();
        let events = &mut value["state"]["npcs"]["2"]["events"];
        let historical = events["assistance"]["history"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find_map(|history| history.get_mut("Enter"))
            .unwrap();
        mutate_payload(historical, case);
        let pending = events["assistance"]["pending"].as_object_mut().unwrap();
        assert_eq!(
            pending.len(),
            1,
            "requires the pending first assistance occurrence"
        );
        mutate_payload(&mut pending.values_mut().next().unwrap()["payload"], case);
        assert_ne!(
            value, base,
            "mutation {case} must alter checkpoint evidence"
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(
                world.profile().clone(),
                life::rehash(value),
            )
            .is_err(),
            "rehashed multi-neighbor mutation {case}"
        );
    }
}

fn mutate_payload(payload: &mut serde_json::Value, case: u32) {
    assert!(payload["traversal"]["candidates"].as_array().unwrap().len() > 2);
    match case {
        // Keep all identities present but reorder the runtime-derived callbacks.
        0 => payload["traversal"]["candidates"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1),
        // This remains a structurally valid non-self denial. The independent
        // Unit-owner anchor must detect rewriting the historical semantic result.
        1 => {
            let candidate = payload["traversal"]["candidates"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|candidate| candidate["npc"]["spawn"] != 2)
                .unwrap();
            assert_eq!(candidate["denial"], "CannotAttackEnemyOnSight");
            candidate["denial"] = "NotFriendlyToOwner".into();
        }
        // Even coordinated removal from the request, provider and receipt cannot
        // turn an incomplete profile inventory into complete historical evidence.
        2 => {
            let id = payload["request"]["scene_inventory"]
                .as_array_mut()
                .unwrap()
                .pop()
                .unwrap();
            remove_candidate(payload, &id);
            rebind_request(payload);
        }
        // An echoed future generation is not an existing historical actor.
        3 => {
            let inventory = payload["request"]["scene_inventory"]
                .as_array_mut()
                .unwrap();
            let id = inventory.iter_mut().find(|id| id["spawn"] != 2).unwrap();
            let old = id.clone();
            id["generation"] = (id["generation"].as_u64().unwrap() + 1).into();
            let changed = id.clone();
            let facts = &mut payload["receivers"]["AuthoredCompleteTraversal"];
            for container in ["grid", "world", "outside_coverage"] {
                for candidate in facts[container].as_array_mut().unwrap() {
                    if *candidate == old {
                        *candidate = changed.clone();
                    }
                }
            }
            for candidate in payload["traversal"]["candidates"].as_array_mut().unwrap() {
                if candidate["npc"] == old {
                    candidate["npc"] = changed.clone();
                }
            }
            rebind_request(payload);
        }
        // Coordinating both order representations still cannot rewrite history.
        4 => {
            payload["receivers"]["AuthoredCompleteTraversal"]["grid"]
                .as_array_mut()
                .unwrap()
                .swap(0, 1);
            payload["traversal"]["candidates"]
                .as_array_mut()
                .unwrap()
                .swap(0, 1);
        }
        5 => payload["traversal"] = serde_json::Value::Null,
        6 => {
            payload["traversal"]["candidates"]
                .as_array_mut()
                .unwrap()
                .pop();
        }
        // Outside coverage is explicit provider evidence, not a way to remove a
        // previously visited neighbor from the retained, anchored traversal.
        7 => {
            let facts = &mut payload["receivers"]["AuthoredCompleteTraversal"];
            let removed = facts["grid"].as_array_mut().unwrap().remove(0);
            assert_ne!(removed["spawn"], 2);
            facts["outside_coverage"]
                .as_array_mut()
                .unwrap()
                .push(removed.clone());
            payload["traversal"]["candidates"]
                .as_array_mut()
                .unwrap()
                .retain(|candidate| candidate["npc"] != removed);
        }
        _ => unreachable!(),
    }
}
fn remove_candidate(payload: &mut serde_json::Value, id: &serde_json::Value) {
    let facts = &mut payload["receivers"]["AuthoredCompleteTraversal"];
    for container in ["grid", "world", "outside_coverage"] {
        facts[container]
            .as_array_mut()
            .unwrap()
            .retain(|candidate| candidate != id);
    }
    payload["traversal"]["candidates"]
        .as_array_mut()
        .unwrap()
        .retain(|candidate| &candidate["npc"] != id);
}
fn rebind_request(payload: &mut serde_json::Value) {
    let request: CombatAssistanceRequest =
        serde_json::from_value(payload["request"].clone()).unwrap();
    payload["receivers"]["AuthoredCompleteTraversal"]["request"] = payload["request"].clone();
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-assistance-traversal-v50\0");
    hash.update(serde_json::to_vec(&request).unwrap());
    let digest: [u8; 32] = hash.finalize().into();
    payload["traversal"]["request"] = serde_json::json!(digest);
}
