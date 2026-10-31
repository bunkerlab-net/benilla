// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual dry owners and queued source notifications; membership and visibility
//! remain authored component inputs, never loaded Northshire combat evidence.
use super::*;

fn delayed_world<'a>(
    snapshot: MapHeightSnapshot<'a>,
    notify_ms: u32,
) -> (NativeWorld, ClusterAuthority<'a>) {
    let mut profile = cluster_profile(snapshot_binding(&snapshot));
    profile
        .source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = notify_ms;
    profile.validate().unwrap();
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &profile, snapshot).unwrap();
    let mut authority = ClusterAuthority::new(provider);
    let world = NativeWorld::synthetic_component_test_with_initialization(
        profile,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    (world, authority)
}

fn peer(reply: &AuthorityReply) -> Option<(&VisitorSightRequest, &PeerSightReceipt)> {
    match (&reply.request.query, &reply.facts) {
        (
            Query::SourceMotion(MotionQuery::VisitorSight(request)),
            Facts::SourceMotion(MotionFacts::VisitorSight(SightDisposition::Peer(receipt))),
        ) => Some((request, receipt)),
        _ => None,
    }
}

fn participant(world: &NativeWorld, spawn: u64) -> VisitorParticipant {
    VisitorParticipant::Creature(world.state().npcs[&spawn].id)
}

fn entry_payload(world: &NativeWorld) -> &AssistancePayload {
    world.state().npcs[&2]
        .events
        .assistance()
        .unwrap()
        .history()
        .iter()
        .find_map(|history| match history {
            AssistanceHistory::Enter(payload) => Some(payload.as_ref()),
            _ => None,
        })
        .unwrap()
}

/// The first real projectile establishes the physical Unit deadline. Fresh
/// profiles then place their ordinary initial notify before, on or after it.
#[test]
fn seven_visible_dry_owners_check_help_across_the_actual_queued_reenable() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let load = || {
        products
            .load_map_height(config, Some(&participants))
            .unwrap()
    };
    let (mut calibration, mut authority) = delayed_world(load(), 10_000);
    enter(&mut calibration, &mut authority);
    let deadline = entry_payload(&calibration).deadline_ms;
    let tick_ms = u64::from(calibration.profile().tick_ms);
    assert_eq!(
        deadline,
        entry_payload(&calibration).request.unit_clock_ms + 2000
    );
    assert!(deadline + tick_ms < 10_000);

    for notify_ms in [deadline - tick_ms, deadline, deadline + tick_ms] {
        let (mut world, mut authority) = delayed_world(load(), notify_ms.try_into().unwrap());
        let initial = world.checkpoint().unwrap();
        let entered = enter(&mut world, &mut authority);
        assert!(assert_evaluation(&world, &entered, 100));
        let payload = entry_payload(&world).clone();
        assert_eq!(payload.deadline_ms, deadline);
        assert!(!world.state().npcs[&2]
            .events
            .assistance()
            .unwrap()
            .can_check_for_help());
        let queue = world.state().npcs[&2].events.queue_snapshot();
        let reenable = queue
            .queued
            .iter()
            .find(|e| e.event_id == payload.reenable_event)
            .unwrap();
        let linking = queue
            .queued
            .iter()
            .find(|e| e.event_id == payload.linking_event)
            .unwrap();
        assert_eq!(
            (reenable.exec_time_ms, linking.exec_time_ms),
            (deadline, deadline)
        );
        assert!(reenable.queue.unwrap().insertion_id < linking.queue.unwrap().insertion_id);

        // No queue mutation or forged AI state: six independently initialized
        // owners retain their original physical AiNotify event throughout combat.
        for spawn in OWNERS.into_iter().filter(|spawn| *spawn != 2) {
            let npc = &world.state().npcs[&spawn];
            let notify = npc
                .events
                .event(npc.events.notify_handle().unwrap())
                .unwrap();
            assert_eq!(notify.event_id, 1);
            assert_eq!(notify.exec_time_ms, notify_ms);
            assert_eq!(npc.ai.phase(), ai::Phase::Idle);
        }
        while world.state().npcs[&6].events.clock_ms() + tick_ms < notify_ms {
            step(&mut world, &mut authority);
        }
        let before = world.checkpoint().unwrap();
        let before_records = world.records().to_vec();
        if notify_ms == deadline {
            // The final notifying owner fails after earlier callbacks and
            // owner 2's actual re-enable/linking execution have been staged.
            authority.dry.text.base.derived.clear();
            authority.dry.text.base.fail_derived_owner = Some(5);
            let error = world
                .step(world.state().tick + 1, None, &mut authority)
                .unwrap_err();
            assert_eq!(
                error,
                NativeError::AuthorityUnavailable("authored late owner derived receipt fault")
            );
            let tentative: Vec<_> = authority
                .dry
                .text
                .base
                .derived
                .iter()
                .filter_map(peer)
                .filter_map(|(_, receipt)| receipt.help.as_ref())
                .collect();
            assert!(tentative
                .iter()
                .any(|help| help.denial == PeerHelpDenial::TargetCannotCheck));
            assert!(tentative
                .iter()
                .any(|help| help.denial == PeerHelpDenial::CannotAttackVictimOnSight));
            assert_eq!(
                world.checkpoint().unwrap().encode().unwrap(),
                before.encode().unwrap()
            );
            assert_eq!(world.records(), before_records);
            authority.dry.text.base.fail_derived_owner = None;
        }
        step(&mut world, &mut authority);
        let record = world.last_tick().unwrap().clone();
        assert_visible_callbacks(&world, &record, &payload, notify_ms);
        if notify_ms == deadline {
            let mut clean = NativeWorld::restore_synthetic_component_test(
                world.profile().clone(),
                before.clone(),
            )
            .unwrap();
            step(&mut clean, &mut authority);
            assert_eq!(clean.last_tick(), Some(&record));
            assert_eq!(clean.checkpoint().unwrap(), world.checkpoint().unwrap());
            reject_forged_help(&world, before, &record);

            // One complete genesis-to-combat replay includes every real dry
            // initialization, both notification directions and queued callbacks.
            let mut replay =
                NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial)
                    .unwrap();
            replay.replay(world.records()).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
            let saved = world.checkpoint().unwrap();
            world.checkpoint_committed(&saved).unwrap();
            let mut restored =
                NativeWorld::restore_synthetic_component_test(world.profile().clone(), saved)
                    .unwrap();
            step(&mut restored, &mut authority);
            step(&mut world, &mut authority);
            assert_eq!(restored.checkpoint().unwrap(), world.checkpoint().unwrap());
        }
        assert!(authority.dry.unsupported_motion.is_empty());
    }
}

fn assert_visible_callbacks(
    world: &NativeWorld,
    record: &TickRecord,
    payload: &AssistancePayload,
    notify_ms: u64,
) {
    let target = participant(world, 2);
    let callbacks: Vec<_> = record
        .replies
        .iter()
        .filter_map(peer)
        .filter(|(request, _)| request.observer == target || request.target == target)
        .collect();
    assert_eq!(callbacks.len(), 12);
    for (pair, spawn) in callbacks
        .as_chunks::<2>()
        .0
        .iter()
        .zip(UPDATE_ORDER.into_iter().filter(|spawn| *spawn != 2))
    {
        let owner = participant(world, spawn);
        let (reverse, reverse_receipt) = pair[0];
        let (request, receipt) = pair[1];
        assert_eq!(
            (reverse.notification_owner, reverse.observer, reverse.target),
            (owner, target, owner)
        );
        assert_eq!(reverse.direction, VisitorDirection::VisitedToOwner);
        assert_eq!(
            reverse_receipt.disposition,
            PeerSightDisposition::ExistingOutdoorVictim
        );
        assert!(reverse_receipt.help.is_none());
        assert_eq!(
            (request.notification_owner, request.observer, request.target),
            (owner, owner, target)
        );
        assert_eq!(request.direction, VisitorDirection::OwnerToVisited);
        assert_eq!(request.container, VisitorContainer::GridCreatures);
        assert_eq!(request.visit_index, 3);
        assert_eq!(request.callback, reverse.callback);
        assert_eq!(request.callback.due.scheduled_ms, notify_ms);
        assert_eq!(
            request.callback.due.now_ms,
            world.state().npcs[&spawn].events.clock_ms()
        );
        assert_eq!(request.callback.queue.clock_ms, request.callback.due.now_ms);
        assert_eq!(receipt.disposition, PeerSightDisposition::NonAggressive);
        let help = receipt.help.as_ref().unwrap();
        let reenabled =
            notify_ms > payload.deadline_ms || (notify_ms == payload.deadline_ms && spawn != 6);
        assert_eq!(help.source, payload.request.source);
        assert_eq!(help.observer, world.state().npcs[&spawn].id);
        assert_eq!(help.target, world.state().npcs[&2].id);
        assert_eq!(help.helper_victim, world.state().player);
        assert!(!help.observer_in_combat);
        assert!(help.observer_can_call && help.target_can_call && help.observer_can_check);
        assert_eq!(help.target_can_check, reenabled);
        assert_eq!(help.target_unit_state, 0);
        assert_ne!(
            help.target_unit_flags & benilla_sim::classic_targets::unit_flags::IN_COMBAT,
            0
        );
        assert_eq!(help.observer_can_initiate, reenabled.then_some(true));
        assert_eq!(help.observer_can_attack_victim, reenabled.then_some(false));
        assert_eq!(
            help.denial,
            if reenabled {
                PeerHelpDenial::CannotAttackVictimOnSight
            } else {
                PeerHelpDenial::TargetCannotCheck
            }
        );
        assert_eq!(world.state().npcs[&spawn].ai.phase(), ai::Phase::Idle);
        assert!(world.state().npcs[&spawn].ai.target().is_none());
    }
    let callbacks: Vec<_> = world.state().npcs[&2]
        .events
        .assistance()
        .unwrap()
        .history()
        .iter()
        .filter_map(|history| match history {
            AssistanceHistory::Callback {
                tick, due, outcome, ..
            } => Some((*tick, *due, *outcome)),
            _ => None,
        })
        .collect();
    if notify_ms >= payload.deadline_ms {
        assert_eq!(callbacks.len(), 2);
        assert_eq!(callbacks[0].1.scheduled_ms, payload.deadline_ms);
        assert_eq!(callbacks[1].1.scheduled_ms, payload.deadline_ms);
        assert!(callbacks[0].1.insertion_id < callbacks[1].1.insertion_id);
        assert_eq!(callbacks[0].2, AssistanceOutcome::Reenabled);
        assert_eq!(
            callbacks[1].2,
            AssistanceOutcome::EmptyContinuation {
                call_assistance: false
            }
        );
        if notify_ms == payload.deadline_ms {
            assert_eq!(callbacks[0].0, record.tick);
            assert_eq!(callbacks[1].0, record.tick);
        }
        assert!(world.state().npcs[&2]
            .events
            .assistance()
            .unwrap()
            .can_check_for_help());
    } else {
        assert!(callbacks.is_empty());
        assert!(!world.state().npcs[&2]
            .events
            .assistance()
            .unwrap()
            .can_check_for_help());
    }
    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    assert!(record
        .events
        .iter()
        .all(|event| !matches!(event, Event::EventAiEvaluation(_))));
}

fn reject_forged_help(world: &NativeWorld, before: Checkpoint, record: &TickRecord) {
    for case in 0..10 {
        let mut forged = record.clone();
        let receipt =
            forged
                .replies
                .iter_mut()
                .find_map(|reply| match &mut reply.facts {
                    Facts::SourceMotion(MotionFacts::VisitorSight(SightDisposition::Peer(
                        receipt,
                    ))) if receipt.help.is_some() => Some(receipt),
                    _ => None,
                })
                .unwrap();
        let help = receipt.help.as_mut().unwrap();
        assert_eq!(help.denial, PeerHelpDenial::TargetCannotCheck);
        match case {
            0 => help.source[0] ^= 1,
            1 => help.observer = help.target,
            2 => help.target.generation += 1,
            3 => help.helper_victim.character[0] ^= 1,
            // Internally consistent alternate denial still contradicts the
            // live pre-reenable Unit owner in this exact callback.
            4 => {
                help.target_can_check = true;
                help.observer_can_initiate = Some(true);
                help.observer_can_attack_victim = Some(false);
                help.denial = PeerHelpDenial::CannotAttackVictimOnSight;
            }
            5 => receipt.help = None,
            6 => receipt.disposition = PeerSightDisposition::ExistingOutdoorVictim,
            7 => {
                help.observer_can_call = false;
                help.denial = PeerHelpDenial::ObserverCannotCall;
            }
            8 => {
                help.observer_can_check = false;
                help.denial = PeerHelpDenial::ObserverCannotCheck;
            }
            9 => help.target_unit_flags = 0,
            _ => unreachable!(),
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), before.clone())
                .unwrap();
        assert!(
            replay.replay(&[forged]).is_err(),
            "forged help receipt {case}"
        );
        assert_eq!(replay.checkpoint().unwrap(), before);
    }
    let reply = record
        .replies
        .iter()
        .find(|reply| peer(reply).is_some_and(|(_, receipt)| receipt.help.is_some()))
        .unwrap();
    for field in [
        "help",
        "observer_can_initiate",
        "observer_can_attack_victim",
    ] {
        let mut value = serde_json::to_value(reply).unwrap();
        let receipt = &mut value["facts"]["SourceMotion"]["VisitorSight"]["Peer"];
        if field == "help" {
            assert!(receipt.as_object_mut().unwrap().remove(field).is_some());
        } else {
            assert!(receipt["help"]
                .as_object_mut()
                .unwrap()
                .remove(field)
                .is_some());
        }
        assert!(
            serde_json::from_value::<AuthorityReply>(value).is_err(),
            "missing required {field}"
        );
    }
}
