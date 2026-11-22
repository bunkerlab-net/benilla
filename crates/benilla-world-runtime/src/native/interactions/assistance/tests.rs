// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored semantic traversal checks; no real map coverage or native admission.
use super::*;
use crate::native::{
    synthetic_fixture as f, synthetic_motion_fixture, synthetic_two_owner_fixture,
};

fn traversal_world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap()
}
fn scene_request(world: &NativeWorld) -> CombatAssistanceRequest {
    let owner = &world.state.npcs[&2];
    let moment = InteractionMoment::Frame { action_ordinal: 1 };
    CombatAssistanceRequest {
        binding: Binding {
            world: world.state.world,
            tick: world.state.tick,
            profile: world.profile.digest(),
            authority_policy: world.profile.authority_policy,
            geometry: world.profile.geometry_revision,
            npc: Some(owner.id),
        },
        content: world.profile.content_digest,
        source: [7; 32],
        generation_source: [6; 32],
        callback: EventAiCallbackKind::CombatReceipt {
            receipt: ai::Receipt {
                id: 1,
                event: ai::CombatEvent::AttackedBy { target: SOLO_UNIT },
            },
        },
        moment,
        now_ms: world.state.now_ms,
        occurrence: 1,
        unit_clock_ms: owner.events.clock_ms(),
        owner_pose: owner.source_motion.as_ref().unwrap().motion.pose(),
        enemy: world.state.player,
        enemy_pose: benilla_sim::creature_motion::Pose {
            position: world.state.position,
            orientation: ai::Real::from_bits(world.state.orientation.bits()).unwrap(),
        },
        radius: f::real(5.0),
        owner_bounding_radius: world.profile.npc(2).unwrap().ai.bounding_radius,
        scene: interactions::witness(world.profile.digest(), &world.state, moment)
            .unwrap()
            .draft,
        scene_inventory: world.state.npcs.values().map(|npc| npc.id).collect(),
        rng_counter: world.state.rng_counter,
        event_ai_callback: None,
    }
}
fn complete_traversal(
    request: &CombatAssistanceRequest,
    grid: &[u64],
    world: &[u64],
    outside: &[u64],
) -> CombatAssistanceFacts {
    let ids = |spawns: &[u64]| {
        spawns
            .iter()
            .map(|spawn| {
                *request
                    .scene_inventory
                    .iter()
                    .find(|npc| npc.spawn == *spawn)
                    .unwrap()
            })
            .collect()
    };
    CombatAssistanceFacts::AuthoredCompleteTraversal {
        request: Box::new(request.clone()),
        grid: ids(grid),
        world: ids(world),
        outside_coverage: ids(outside),
    }
}
fn derive(
    world: &NativeWorld,
    request: &CombatAssistanceRequest,
    facts: &CombatAssistanceFacts,
) -> Result<AssistanceTraversalReceipt, NativeError> {
    interactions::source_assistance_traversal(&world.profile, &world.state, request, facts)
}
fn friendly_defensive_profile(hostile_to_player: bool) -> Profile {
    let mut profile = synthetic_two_owner_fixture::profile();
    // Raw faction friendship to the caller and player hostility are independent.
    let faction = profile
        .interactions
        .as_mut()
        .unwrap()
        .factions
        .get_mut(&3)
        .unwrap();
    faction.friend_group_mask = 2;
    faction.enemy_group_mask = if hostile_to_player { 1 } else { 0 };
    profile.npcs[1].ai.reaction = ai::Reaction::Defensive;
    profile
        .initialize_combat_authority(
            profile
                .npcs
                .iter()
                .map(|npc| NpcCombatAuthoritySource {
                    entry: npc.entry,
                    extra_flags: if npc.ai.creature_id == 3 { 2 } else { 0 },
                    static_flags1: 0,
                    static_flags2: 0,
                    static_flags3: 0,
                    static_flags4: 0,
                })
                .collect(),
        )
        .unwrap();
    profile
}
/// Realize the oracle's explicitly authored range leaf with exact runtime poses.
/// The source threshold is radius + (owner reach + candidate reach), with a strict
/// comparison. Equality denies; one binary32 ULP inside admits the LOS continuation.
/// These positions do not turn the oracle's authored range leaf into geometry proof.
fn with_candidate_range(mut profile: Profile, radius: ai::Real, within: bool) -> Profile {
    let reaches = profile.npc(2).unwrap().ai.combat_reach.as_f32()
        + profile.npc(3).unwrap().ai.combat_reach.as_f32();
    let threshold = radius.as_f32() + reaches;
    assert!(threshold > 0.0);
    let distance = if within {
        f32::from_bits(threshold.to_bits() - 1)
    } else {
        threshold
    };
    profile
        .npcs
        .iter_mut()
        .find(|npc| npc.ai.creature_id == 2)
        .unwrap()
        .ai
        .spawn = ai::Point::new(0.0, 0.0, 0.0).unwrap();
    profile
        .npcs
        .iter_mut()
        .find(|npc| npc.ai.creature_id == 3)
        .unwrap()
        .ai
        .spawn = ai::Point::new(distance, 0.0, 0.0).unwrap();
    profile.validate().unwrap();
    profile
}
fn mark_in_combat(world: &mut NativeWorld, spawn: u64) {
    let npc = &world.state.npcs[&spawn];
    let mut observation = ai::Observation {
        sequence: npc.ai.next_sequence().unwrap(),
        elapsed_ms: world.profile.tick_ms,
        geometry_revision: ai::movement::Revision(world.profile.geometry_revision),
        target: Some(ai::Target {
            id: SOLO_UNIT,
            position: npc.ai.position(),
            level: world.profile.player.level,
            combat_reach: world.profile.player.combat_reach,
            bounding_radius: world.profile.player.bounding_radius,
            moving_and_running: false,
            alive: true,
            online: true,
            can_attack_in_combat: true,
            can_attack_on_sight: true,
            visible: true,
            accessible: true,
            line_of_sight: true,
        }),
        can_initiate_attack: true,
        casting_non_melee: false,
        stationary_melee_refresh_eligible: false,
        creature_moving_and_running: false,
        movement: None,
        receipts: vec![],
    };
    observation.receipts.push(ai::Receipt {
        id: 1,
        event: ai::CombatEvent::AttackedBy { target: SOLO_UNIT },
    });
    world
        .state
        .npcs
        .get_mut(&spawn)
        .unwrap()
        .ai
        .source_combat_event(&observation)
        .unwrap();
}
#[test]
fn complete_partition_rejects_omissions_duplicates_unknown_stale_and_self_outside() {
    let world = traversal_world(synthetic_two_owner_fixture::profile());
    let request = scene_request(&world);
    assert!(request
        .validate(&CombatAssistanceFacts::AuthoredCompleteEmptyTraversal {
            request: Box::new(request.clone()),
            self_container: AssistanceSelfContainer::Grid,
        })
        .is_err());
    for (grid, world, outside) in [
        (vec![2], vec![], vec![]),
        (vec![2, 3], vec![3], vec![]),
        (vec![3], vec![], vec![2]),
        (vec![2], vec![], vec![2]),
    ] {
        assert!(request
            .validate(&complete_traversal(&request, &grid, &world, &outside))
            .is_err());
    }
    let facts = complete_traversal(&request, &[2], &[3], &[]);
    for mutate in [(false, false), (true, false), (false, true)] {
        let mut changed = facts.clone();
        let CombatAssistanceFacts::AuthoredCompleteTraversal {
            world: visits,
            request,
            ..
        } = &mut changed
        else {
            unreachable!()
        };
        if mutate.0 {
            visits[0].generation += 1;
        } else if mutate.1 {
            request.scene_inventory[1].generation += 1;
        } else {
            visits[0].spawn = 999;
        }
        assert!(derive(&world, &scene_request(&world), &changed).is_err());
    }
    let mut stale = world.state.clone();
    stale.npcs.get_mut(&3).unwrap().id.generation += 1;
    let mut changed_world = world;
    changed_world.state = stale;
    let changed_request = scene_request(&changed_world);
    let changed_facts = complete_traversal(&changed_request, &[2, 3], &[], &[]);
    assert!(derive(&changed_world, &changed_request, &changed_facts).is_err());
}
#[test]
fn provider_membership_never_supplies_semantics_and_outside_is_explicit() {
    let world = traversal_world(synthetic_two_owner_fixture::profile());
    let request = scene_request(&world);
    let facts = complete_traversal(&request, &[3], &[2], &[]);
    let before = world.state.clone();
    let receipt = derive(&world, &request, &facts).unwrap();
    assert_eq!(
        receipt
            .candidates
            .iter()
            .map(|r| (r.container, r.npc.spawn, r.denial))
            .collect::<Vec<_>>(),
        vec![
            (
                AssistanceSelfContainer::Grid,
                3,
                AssistanceDenial::NotFriendlyToOwner
            ),
            (
                AssistanceSelfContainer::World,
                2,
                AssistanceDenial::SelfCandidate
            ),
        ]
    );
    assert_eq!(world.state, before);
    let outside = complete_traversal(&request, &[2], &[], &[3]);
    assert_eq!(
        derive(&world, &request, &outside).unwrap().candidates.len(),
        1
    );
    let mut changed_receipt = receipt.clone();
    changed_receipt.candidates.reverse();
    assert!(changed_receipt.validate(&request, &facts).is_err());
    let reordered = complete_traversal(&request, &[2], &[3], &[]);
    assert!(receipt.validate(&request, &reordered).is_err());
    let mut injected = serde_json::to_value(&facts).unwrap();
    injected["AuthoredCompleteTraversal"]["denials"] = serde_json::json!(["SelfCandidate"]);
    assert!(serde_json::from_value::<CombatAssistanceFacts>(injected).is_err());
}
#[test]
fn defensive_candidate_neutral_to_enemy_denies_but_hostile_reaches_unsupported_range_los() {
    for hostile in [false, true] {
        for within in [false, true] {
            let profile =
                with_candidate_range(friendly_defensive_profile(hostile), f::real(5.0), within);
            let world = traversal_world(profile);
            let request = scene_request(&world);
            let facts = complete_traversal(&request, &[2, 3], &[], &[]);
            let before = world.state.clone();
            let result = derive(&world, &request, &facts);
            if hostile && within {
                assert_eq!(
                    result,
                    Err(NativeError::UnsupportedMechanic(
                        "assistance candidate reached LOS"
                    ))
                );
            } else {
                assert_eq!(
                    result.unwrap().candidates[1].denial,
                    if hostile {
                        AssistanceDenial::OutOfRange
                    } else {
                        AssistanceDenial::CannotAttackEnemyOnSight
                    }
                );
            }
            assert_eq!(world.state, before);
            assert_eq!(world.profile.npcs[1].ai.reaction, ai::Reaction::Defensive);
        }
    }
}
#[test]
fn source_order_is_self_then_combat_then_owner_flags_then_faction_then_enemy() {
    let mut profile = friendly_defensive_profile(true);
    profile.npcs[0].health_regeneration.unit_flags = 0x0200_0000;
    let owner_entry = profile.npcs[0].entry;
    profile
        .interactions
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&owner_entry)
        .unwrap()
        .unit_flags = 0x0200_0000;
    let mut world = traversal_world(profile);
    let request = scene_request(&world);
    let facts = complete_traversal(&request, &[2, 3], &[], &[]);
    let receipt = derive(&world, &request, &facts).unwrap();
    assert_eq!(
        receipt.candidates[0].denial,
        AssistanceDenial::SelfCandidate
    );
    assert_eq!(
        receipt.candidates[1].denial,
        AssistanceDenial::OwnerUninteractible
    );
    mark_in_combat(&mut world, 3);
    let request = scene_request(&world);
    let facts = complete_traversal(&request, &[2, 3], &[], &[]);
    assert_eq!(
        derive(&world, &request, &facts).unwrap().candidates[1].denial,
        AssistanceDenial::AlreadyInCombat
    );
}

fn same_entry_scene() -> NativeWorld {
    let mut profile = synthetic_two_owner_fixture::profile();
    let mut duplicate = profile.npcs[0].clone();
    duplicate.ai.creature_id = 4;
    profile.npcs.push(duplicate);
    let motion = profile.source_motion.as_mut().unwrap();
    motion.npcs.insert(4, motion.npcs[&2].clone());
    profile.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(
            synthetic_motion_fixture::creature_capability_sources(&profile),
        )
        .unwrap(),
    );
    profile.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(synthetic_two_owner_fixture::visitor_input(
            &profile,
            vec![2, 3, 4],
        ))
        .unwrap(),
    );
    profile.validate().unwrap();
    traversal_world(profile)
}
#[test]
fn same_entry_is_not_self_and_mixed_containers_preserve_callback_order() {
    let mut world = same_entry_scene();
    let request = scene_request(&world);
    let facts = complete_traversal(&request, &[2, 4], &[3], &[]);
    assert_eq!(world.profile.npcs[0].entry, world.profile.npcs[2].entry);
    assert_eq!(
        derive(&world, &request, &facts),
        Err(NativeError::UnsupportedMechanic(
            "assistance candidate reached LOS",
        ))
    );
    mark_in_combat(&mut world, 4);
    let request = scene_request(&world);
    let facts = complete_traversal(&request, &[4, 3], &[2], &[]);
    let before = world.state.clone();
    let receipt = derive(&world, &request, &facts).unwrap();
    assert_eq!(
        receipt
            .candidates
            .iter()
            .map(|r| (r.container, r.npc.spawn, r.denial))
            .collect::<Vec<_>>(),
        vec![
            (
                AssistanceSelfContainer::Grid,
                4,
                AssistanceDenial::AlreadyInCombat
            ),
            (
                AssistanceSelfContainer::Grid,
                3,
                AssistanceDenial::NotFriendlyToOwner
            ),
            (
                AssistanceSelfContainer::World,
                2,
                AssistanceDenial::SelfCandidate
            ),
        ]
    );
    assert_eq!(world.state, before);
    let mut reordered = receipt.clone();
    reordered.candidates.swap(0, 1);
    assert!(reordered.validate(&request, &facts).is_err());
}
#[test]
fn current_scene_changes_incomplete_overlays_and_receipt_mutations_reject() {
    let world = traversal_world(friendly_defensive_profile(false));
    let request = scene_request(&world);
    let facts = complete_traversal(&request, &[2, 3], &[], &[]);
    let receipt = derive(&world, &request, &facts).unwrap();
    for mutation in [0, 1, 2, 3, 4] {
        let mut state = world.state.clone();
        match mutation {
            0 => {
                state.npcs.remove(&2);
            }
            1 => {
                state.npcs.remove(&3);
            }
            2 => {
                state.health -= 1;
            }
            3 => {
                state.npcs.get_mut(&3).unwrap().health -= 1;
            }
            4 => {
                let candidate = state.npcs.remove(&3).unwrap();
                state.npcs.insert(99, candidate);
            }
            _ => unreachable!(),
        }
        assert!(interactions::source_assistance_traversal(
            &world.profile,
            &state,
            &request,
            &facts
        )
        .is_err());
    }
    for mutation in [0, 1, 2, 3] {
        let mut changed = receipt.clone();
        match mutation {
            0 => {
                changed.candidates.pop();
            }
            1 => {
                changed.candidates[1].npc.generation += 1;
            }
            2 => {
                changed.candidates[1].denial = AssistanceDenial::SelfCandidate;
            }
            3 => {
                changed.request[0] ^= 1;
            }
            _ => unreachable!(),
        }
        assert!(changed.validate(&request, &facts).is_err());
    }
    // A historical receipt remains about its captured draft, not later combat.
    let mut later = world;
    mark_in_combat(&mut later, 3);
    receipt.validate(&request, &facts).unwrap();
    assert!(derive(&later, &request, &facts).is_err());
    let current = scene_request(&later);
    let facts = complete_traversal(&current, &[2, 3], &[], &[]);
    assert_eq!(
        derive(&later, &current, &facts).unwrap().candidates[1].denial,
        AssistanceDenial::AlreadyInCombat
    );
}

#[test]
fn original_cpp_candidate_vectors_match_runtime_semantic_prefix() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../scripts/native_assistance_source_oracle/candidate_vectors.json"
    ))
    .unwrap();
    assert_eq!(
        vectors["source_revision"],
        "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
    );
    for (name, expected_denial) in [
        (
            "self_rejected_before_all_predicates",
            Some(AssistanceDenial::SelfCandidate),
        ),
        (
            "outdoor_in_combat_rejected_before_relation",
            Some(AssistanceDenial::AlreadyInCombat),
        ),
        (
            "owner_uninteractible_rejected_before_relation",
            Some(AssistanceDenial::OwnerUninteractible),
        ),
        (
            "candidate_neutral_to_owner_rejected",
            Some(AssistanceDenial::NotFriendlyToOwner),
        ),
        (
            "candidate_neutral_to_player_attackable_not_enemy",
            Some(AssistanceDenial::CannotAttackEnemyOnSight),
        ),
        ("hostile_defensive_eligible", None),
        ("same_entry_distinct_guid_eligible", None),
        (
            "range_failure_skips_los",
            Some(AssistanceDenial::OutOfRange),
        ),
        (
            "semantic_denial_skips_false_range_and_los",
            Some(AssistanceDenial::AlreadyInCombat),
        ),
    ] {
        let case = vectors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let input = &case["input"];
        assert_eq!(input["instanceable"], false);
        let units = input["authored_units"].as_array().unwrap();
        let unit = |guid: u64| units.iter().find(|unit| unit["guid"] == guid).unwrap();
        let mut profile = synthetic_two_owner_fixture::profile();
        // Stable runtime IDs differ from the scalar oracle's object addresses.
        // Entry is not read by these predicates; distinct-entry fixtures map the
        // exact raw relation inputs, with same-entry identity tested separately.
        for (guid, faction_template, spawn) in [(1, 2, Some(2)), (77, 1, None), (101, 3, Some(3))] {
            let unit = unit(guid);
            assert_eq!(unit["has_faction"], true);
            assert_eq!(unit["evading_home"], false);
            assert_eq!(unit["feigning"], false);
            assert_eq!(unit["ignoring_feign"], false);
            assert_eq!(unit["pet_actions_disabled"], false);
            assert_eq!(unit["authored_unit_state"], 0);
            let raw = &unit["faction"];
            let faction = profile
                .interactions
                .as_mut()
                .unwrap()
                .factions
                .get_mut(&faction_template)
                .unwrap();
            faction.faction = raw["faction"].as_u64().unwrap() as u32;
            faction.group_mask = raw["group_mask"].as_u64().unwrap() as u32;
            faction.enemy_group_mask = raw["enemy_mask"].as_u64().unwrap() as u32;
            faction.friend_group_mask = raw["friend_mask"].as_u64().unwrap() as u32;
            faction.enemies =
                std::array::from_fn(|index| raw["enemies"][index].as_u64().unwrap() as u32);
            faction.friends =
                std::array::from_fn(|index| raw["friends"][index].as_u64().unwrap() as u32);
            faction.reputation_list_id = Some(-1);
            if let Some(spawn) = spawn {
                let npc = profile
                    .npcs
                    .iter_mut()
                    .find(|npc| npc.ai.creature_id == spawn)
                    .unwrap();
                npc.ai.neutral_to_all = faction.faction == 0
                    || (faction.enemies.iter().all(|enemy| *enemy == 0)
                        && faction.enemy_group_mask == 0
                        && faction.friend_group_mask == 0);
                npc.ai.reaction = if unit["raw_extra_flags"].as_u64().unwrap() & 2 != 0 {
                    ai::Reaction::Defensive
                } else {
                    ai::Reaction::Aggressive
                };
                let flags = unit["flags"].as_u64().unwrap() as u32 & !0x0008_0000;
                npc.health_regeneration.unit_flags = flags;
                profile
                    .interactions
                    .as_mut()
                    .unwrap()
                    .npcs
                    .get_mut(&npc.entry)
                    .unwrap()
                    .unit_flags = flags;
            } else {
                assert_eq!(unit["flags"], 8);
            }
        }
        profile
            .initialize_combat_authority(
                profile
                    .npcs
                    .iter()
                    .map(|npc| NpcCombatAuthoritySource {
                        entry: npc.entry,
                        extra_flags: unit(if npc.ai.creature_id == 2 { 1 } else { 101 })
                            ["raw_extra_flags"]
                            .as_u64()
                            .unwrap() as u32,
                        static_flags1: 0,
                        static_flags2: 0,
                        static_flags3: 0,
                        static_flags4: 0,
                    })
                    .collect(),
            )
            .unwrap();
        let radius = ai::Real::from_bits(input["radius_bits"].as_u64().unwrap() as u32).unwrap();
        let profile = with_candidate_range(
            profile,
            radius,
            unit(101)["authored_within_range"].as_bool().unwrap(),
        );
        let mut world = traversal_world(profile);
        if unit(101)["flags"].as_u64().unwrap() & 0x0008_0000 != 0 {
            mark_in_combat(&mut world, 3);
        }
        let mut request = scene_request(&world);
        request.radius = radius;
        let candidate_sequence = input["authored_candidate_sequence"].as_array().unwrap();
        assert_eq!(candidate_sequence.len(), 1);
        let (candidate, facts) = if candidate_sequence[0] == 1 {
            (2, complete_traversal(&request, &[2], &[], &[3]))
        } else {
            assert_eq!(candidate_sequence[0], 101);
            (3, complete_traversal(&request, &[2, 3], &[], &[]))
        };
        let source = &case["observation"]["evaluations"][0];
        assert_eq!(
            source["accepted"].as_bool().unwrap(),
            expected_denial.is_none(),
            "{name}"
        );
        let actual = derive(&world, &request, &facts);
        if let Some(expected) = expected_denial {
            let actual = actual.unwrap();
            assert_eq!(
                actual
                    .candidates
                    .iter()
                    .find(|receipt| receipt.npc.spawn == candidate)
                    .unwrap()
                    .denial,
                expected,
                "{name}"
            );
            let leaves = source["leaves"].as_array().unwrap();
            assert!(
                leaves
                    .iter()
                    .all(|leaf| !leaf.as_str().unwrap().starts_with("los_query:")),
                "{name}"
            );
            assert_eq!(
                leaves
                    .iter()
                    .any(|leaf| leaf.as_str().unwrap().starts_with("range_query:")),
                expected == AssistanceDenial::OutOfRange,
                "{name}"
            );
            if expected == AssistanceDenial::OutOfRange {
                assert!(
                    leaves.iter().any(|leaf| leaf == "authored_range:101:false"),
                    "{name}"
                );
            }
        } else {
            assert_eq!(
                actual,
                Err(NativeError::UnsupportedMechanic(
                    "assistance candidate reached LOS"
                )),
                "{name}"
            );
            assert!(
                source["leaves"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|leaf| leaf.as_str().unwrap().starts_with("range_query:")),
                "{name}"
            );
        }
    }
}
